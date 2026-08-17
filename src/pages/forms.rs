//! PMS-731: `/admin/forms`, the request-form builder.
//!
//! An admin defines the forms clients fill in: the ordered field set, each
//! field's type and validation, the cross-field rules, and the KB article that
//! documents how to perform the request. mokosh-server owns the validation
//! (`src/modules/forms/`); this page authors the definition and lets the
//! server be the judge of it.
//!
//! One editor rather than the parent-then-children split `sla.rs` uses,
//! because the server requires at least one field to CREATE a definition, and
//! because a PATCH carrying `fields` REPLACES the whole set (field identity is
//! the payload key, so a merge cannot express a rename). A whole-form editor
//! matches both facts exactly.

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::components::{
    use_page_title, ArrowDownIcon, ArrowUpIcon, Badge, BadgeVariant, Button, ButtonVariant,
    Checkbox, ChevronDownIcon, ChevronRightIcon, DataTable, DragHandleIcon, ErrorBanner,
    IconButton, IconSize, Input, PageHeader, Select, SelectOption, Table, TableBody, TableCell,
    TableEmpty, TableHead, TableHeader, TableLoading, TableRow, Textarea, TrashIcon,
};
use crate::modules::forms::{
    CreateFormDefinitionRequest, FieldType, FormDefinition, FormRule, UpdateFormDefinitionRequest,
    UpsertFormField,
};
use crate::modules::kb::KbArticle;
// PMS-744: the preview renders through the client page's own component and
// DTOs, so what an operator signs off on is what a client is served.
use crate::pages::request_form::{PublicField, PublicForm, PublicRule, RequestFormBody};
use crate::utils::Paginated;
use crate::Route;

/// Articles offered in the procedure picker. Definitions are few and the
/// picker is a plain `Select` per docs/form-conventions.md (a tenant's
/// published article set is bounded, unlike companies).
const ARTICLE_PAGE_SIZE: usize = 200;

#[component]
pub fn FormsBuilderPage() -> Element {
    use_page_title("Request Forms");

    let mut forms = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _reachable = crate::hooks::use_server_reachable();
        let _token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_authed_typed::<Vec<FormDefinition>>("/forms")
            .await
            .ok()
    });

    // PMS-759: the caller's own drafts, so a half-built form is findable
    // rather than only ever stumbled on by reopening the right row. Loaded
    // here rather than inside the editor because both surfaces need it and the
    // restore decision has to be made once, at open time.
    let mut drafts = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let _token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_authed_typed::<Vec<ServerDraft>>("/forms/drafts")
            .await
            .ok()
    });

    let mut editing = use_signal(|| None::<EditorState>);
    // MAPPS-424: (definition id, name) of the form being sent, if any.
    let mut sending = use_signal(|| None::<(String, String)>);
    // PMS-764: bumped when a send completes, so the Recently sent panel picks
    // up the link that was just issued. A counter rather than a handle on the
    // panel's resource, so the panel keeps owning its own fetch.
    let mut sent_reload = use_signal(|| 0u32);
    // PMS-744: the client's view of a saved definition, opened from its row.
    let mut previewing = use_signal(|| None::<PublicForm>);
    // MAPPS-436: (draft id, label) of the draft the Discard dialog is asking
    // about. Set by the row button; the DELETE fires from `onconfirm`.
    let mut pending_draft_discard = use_signal(|| None::<(String, String)>);

    let snap = forms.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<FormDefinition> = match &*snap {
        Some(Some(list)) => list.clone(),
        _ => Vec::new(),
    };
    let total = rows.len();

    let draft_rows: Vec<ServerDraft> = match &*drafts.read_unchecked() {
        Some(Some(list)) => list.clone(),
        _ => Vec::new(),
    };

    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    // PMS-748: the client's page names the MSP, so the preview of it has to
    // name the same MSP. PMS-752: and it has to be the same VALUE the email
    // uses, which is mokosh's own `tenants.name`.
    let (tenant_name, tenant_logo) = use_org_identity();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Request Forms".to_string() }
        };
    }

    let pending_discard = pending_draft_discard.read().clone();

    rsx! {
        PageHeader { title: "Request Forms".to_string(),
            // MAPPS-424: the subtitle says what a definition is FOR, because
            // defining one is never the goal. Without it the page ends at Save
            // with no sign that a form reaches a client from somewhere else.
            subtitle: "Forms clients fill in to raise a request. Each one becomes a ticket carrying its knowledge base article. Define a form once here, then use Send to email a client a link to it.".to_string(),
        }

        // PMS-752: the name a client reads on every form and email this page
        // sends. It was reachable only from Settings, three levels into a hub
        // nobody opens while building a form, so a tenant still called "My
        // workspace" emailed clients under that name indefinitely.
        //
        // Read-only here, linking to the page that edits it. Two edit surfaces
        // for one value is how they drift, and the settings page already does
        // it properly.
        if !tenant_name.is_empty() {
            p { class: "-mt-3 mb-4 text-sm text-muted",
                "Clients see these as coming from "
                span { class: "font-medium text-content", "{tenant_name}" }
                ". "
                Link {
                    to: Route::SettingsOrganization {},
                    class: "underline text-accent hover:opacity-90",
                    "Change"
                }
            }
        }

        div { class: "mb-4 flex justify-end",
            Button {
                variant: ButtonVariant::Primary,
                disabled: !can_mutate,
                title: (!can_mutate).then(|| "Can't create a form while the server is unreachable".to_string()),
                onclick: move |_| editing.set(Some(EditorState::new())),
                "New Form"
            }
        }

        if fetch_failed {
            ErrorBanner { class: "mb-3", "Could not load request forms. Refresh the page to retry." }
        }

        // PMS-759: unfinished work, above the saved forms rather than beside
        // them. A draft is not a form: it cannot be sent, it is visible only to
        // the person who wrote it, and it is here to be finished or thrown
        // away. Hidden entirely when there are none, because an empty
        // "Drafts (0)" panel is a permanent reminder of nothing.
        if !draft_rows.is_empty() {
            div { class: "mb-4 rounded-md border border-line bg-surface-2 p-3",
                p { class: "mb-2 text-sm font-medium text-content",
                    "Unfinished drafts"
                }
                p { class: "mb-3 text-xs text-muted",
                    "Only you can see these. They cannot be sent to a client until you save them as a form."
                }
                ul { class: "space-y-2",
                    for draft in draft_rows.iter().cloned() {
                        {
                            let key = draft.id.clone();
                            let label = draft.label();
                            let for_resume = draft.clone();
                            let existing = draft
                                .form_definition_id
                                .as_ref()
                                .and_then(|id| rows.iter().find(|f| f.id.to_string() == *id).cloned());
                            let delete_id = draft.id.clone();
                            let delete_label = draft.label();
                            rsx! {
                                li { key: "{key}",
                                    class: "flex items-center justify-between gap-3 rounded border border-line bg-surface px-3 py-2",
                                    div { class: "min-w-0",
                                        span { class: "text-sm font-medium text-content", "{label}" }
                                        // Which form it belongs to, when it
                                        // belongs to one. Without this a draft
                                        // named the same as its form reads as a
                                        // duplicate of it.
                                        if let Some(def) = existing.as_ref() {
                                            span { class: "ml-2 text-xs text-muted", "edit of {def.name}" }
                                        } else {
                                            span { class: "ml-2 text-xs text-muted", "never saved" }
                                        }
                                    }
                                    div { class: "flex items-center gap-2 shrink-0",
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            disabled: !can_mutate,
                                            onclick: move |_| {
                                                // Resume opens the editor on
                                                // the form the draft belongs
                                                // to, or on an empty one when
                                                // it never became a form. The
                                                // modal picks the newer of this
                                                // draft and any local copy, the
                                                // same as opening the row would.
                                                let base = match existing.as_ref() {
                                                    Some(def) => EditorState::from_existing(def),
                                                    None => EditorState::new(),
                                                };
                                                editing.set(Some(for_resume.payload.clone().into_state(&base)));
                                            },
                                            "Resume"
                                        }
                                        Button {
                                            variant: ButtonVariant::Link,
                                            disabled: !can_mutate,
                                            // MAPPS-436: opens the dialog; the DELETE fires from
                                            // its `onconfirm` below.
                                            onclick: move |_| {
                                                pending_draft_discard
                                                    .set(Some((delete_id.clone(), delete_label.clone())));
                                            },
                                            "Discard"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // MAPPS-436: the DELETE fires from `onconfirm` only.
        if let Some((discard_id, discard_label)) = pending_discard {
            crate::components::ConfirmDialog {
                open: true,
                title: "Discard draft".to_string(),
                message: format!("Discard the unfinished draft \"{discard_label}\"? It cannot be recovered."),
                confirm_text: "Discard".to_string(),
                cancel_text: "Cancel".to_string(),
                destructive: true,
                onconfirm: move |_| {
                    let id = discard_id.clone();
                    pending_draft_discard.set(None);
                    spawn(async move {
                        match crate::hooks::fetch::api::delete_authed(&format!("/forms/drafts/{id}"))
                            .await
                        {
                            Ok(()) => drafts.restart(),
                            // MAPPS-436: a failed discard used to leave the row
                            // in place with no explanation.
                            Err(err) => crate::hooks::push_toast(
                                crate::components::AlertType::Error,
                                format!("Could not discard the draft: {err}"),
                            ),
                        }
                    });
                },
                oncancel: move |_| pending_draft_discard.set(None),
            }
        }

        DataTable {
            loading: is_loading,
            total_items: total,
            current_page: 1,
            per_page: total.max(1),
            columns: 5,
            Table {
                TableHead {
                    TableRow {
                        TableHeader { "Name" }
                        TableHeader { "Link name" }
                        TableHeader { "Procedure" }
                        TableHeader { "Fields" }
                        TableHeader { "Actions" }
                    }
                }
                if is_loading {
                    TableLoading { columns: 5, rows: 4 }
                } else if rows.is_empty() {
                    TableEmpty {
                        columns: 5,
                        title: "No request forms yet".to_string(),
                        description: "Use New Form above to define what a client is asked for when they raise a request.".to_string(),
                    }
                } else {
                    TableBody {
                        for def in rows.into_iter() {
                            {
                                let key = def.id.to_string();
                                let for_edit = def.clone();
                                let article = def.kb_article_title.clone().unwrap_or_default();
                                let field_count = def.fields.len();
                                let active = def.is_active;
                                let sendable = is_sendable(&def);
                                let send_id = def.id.to_string();
                                let send_name = def.name.clone();
                                // PMS-744: mapped once per row rather than in
                                // the click handler, so the closure carries a
                                // ready value instead of the whole definition.
                                let for_preview = preview_from_definition(&def, &tenant_name, tenant_logo.as_deref());
                                rsx! {
                                    TableRow { key: "{key}",
                                        TableCell {
                                            span { class: "font-medium text-content", "{def.name}" }
                                            if !active {
                                                Badge { variant: BadgeVariant::Gray, class: "ml-2".to_string(), "Retired" }
                                            }
                                        }
                                        TableCell { class: "text-muted font-mono text-xs", "{def.slug}" }
                                        TableCell { class: "text-muted",
                                            if article.is_empty() {
                                                span { class: "text-muted italic", "None" }
                                            } else {
                                                "{article}"
                                            }
                                        }
                                        TableCell { class: "text-muted", "{field_count}" }
                                        TableCell {
                                            Button {
                                                variant: ButtonVariant::Link,
                                                onclick: move |_| editing.set(Some(EditorState::from_existing(&for_edit))),
                                                "Edit"
                                            }
                                            // PMS-744: also offered on the row,
                                            // not only inside the editor, so
                                            // "what does this one look like?"
                                            // does not require opening an edit
                                            // form you did not intend to change.
                                            // Shown for a retired form too:
                                            // looking at one is harmless, and
                                            // it is exactly what you want
                                            // before bringing it back.
                                            Button {
                                                variant: ButtonVariant::Link,
                                                onclick: move |_| previewing.set(Some(for_preview.clone())),
                                                "Preview"
                                            }
                                            // MAPPS-424: the entry point into the
                                            // send flow from the side the user is
                                            // already on. Hidden, not disabled,
                                            // for a retired form: the server
                                            // refuses submissions on one, so the
                                            // link would die on arrival.
                                            if sendable {
                                                Button {
                                                    variant: ButtonVariant::Link,
                                                    disabled: !can_mutate,
                                                    title: (!can_mutate).then(|| "Can't send while the server is unreachable".to_string()),
                                                    onclick: move |_| sending.set(Some((send_id.clone(), send_name.clone()))),
                                                    "Send"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // PMS-764: what became of the forms already sent. Until this, a sent
        // link showed up in exactly one place, the company detail page, and
        // nothing here led there: you sent a form, got a toast, and this page
        // looked exactly as it had before.
        div { class: "mt-6",
            crate::pages::request_links::SentRequestLinksPanel { reload: sent_reload }
        }

        if let Some(state) = editing.read().clone() {
            {
                // The draft keyed to whatever is being edited: the one with a
                // matching definition id, or the new-form one. `None` when the
                // list has not loaded, which means the editor falls back to the
                // local copy alone rather than waiting on the network to show
                // someone their own typing.
                let mine = draft_rows
                    .iter()
                    .find(|d| d.form_definition_id == state.id)
                    .cloned();
                rsx! {
                    FormEditorModal {
                        state,
                        server_draft: mine,
                        onclose: move |_| { editing.set(None); drafts.restart(); },
                        onsaved: move |_| {
                            editing.set(None);
                            forms.restart();
                            drafts.restart();
                        },
                    }
                }
            }
        }

        if let Some((id, name)) = sending.read().clone() {
            crate::pages::request_links::SendFormToClientModal {
                form_definition_id: id,
                form_name: name,
                onclose: move |_| { sending.set(None); },
                onsent: move |_| {
                    sending.set(None);
                    // PMS-764: the row appears in the panel below the moment it
                    // is created, which is when "where did that go?" is actually
                    // being asked.
                    sent_reload += 1;
                },
            }
        }

        if let Some(def) = previewing.read().clone() {
            ClientPreviewModal {
                def,
                // Saved definition, so nothing here is a draft; the editor's
                // preview says otherwise for its own unsaved state.
                unsaved: false,
                onclose: move |_| { previewing.set(None); },
            }
        }
    }
}

/// Whether a definition can be sent to a client right now.
///
/// MAPPS-424: retired definitions are excluded because mokosh-server refuses
/// submissions against one, so issuing a link for it would hand the client a
/// page that rejects them. Mirrors the `?active_only=true` filter the send
/// modal already applies to its own form picker.
fn is_sendable(def: &FormDefinition) -> bool {
    def.is_active
}

// ============================================================================
// EDITOR STATE
// ============================================================================

/// One field row being edited. Numeric bounds and the option set are held as
/// strings because that is what the inputs produce; they are parsed on save,
/// so a half-typed "1" never has to round-trip through an `Option<i32>`.
#[derive(Clone, Debug, PartialEq)]
struct FieldRow {
    name: String,
    label: String,
    help_text: String,
    field_type: FieldType,
    is_required: bool,
    max_length: String,
    /// Comma-separated; split on save. A textarea would invite newlines that
    /// then have to be stripped, and an option set is short by nature.
    options: String,
    date_not_in_past: bool,
    /// PMS-747: whether the operator has taken the reference name over. Until
    /// they do it follows the label, the way the form's link name follows the
    /// form name. Set on an existing definition so a label typo fixed months
    /// later cannot silently rekey a live field.
    name_touched: bool,
}

impl FieldRow {
    fn new() -> Self {
        Self {
            name: String::new(),
            label: String::new(),
            help_text: String::new(),
            field_type: FieldType::Text,
            is_required: false,
            max_length: String::new(),
            options: String::new(),
            date_not_in_past: false,
            name_touched: false,
        }
    }

    fn parsed_options(&self) -> Vec<String> {
        self.options
            .split(',')
            .map(|o| o.trim().to_string())
            .filter(|o| !o.is_empty())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RuleRow {
    field: String,
    when_field: String,
    equals: String,
}

#[derive(Clone, Debug, PartialEq)]
struct EditorState {
    id: Option<String>,
    name: String,
    slug: String,
    description: String,
    /// PMS-748: how a client reaches the MSP about this request.
    contact_info: String,
    kb_article_id: String,
    is_active: bool,
    fields: Vec<FieldRow>,
    rules: Vec<RuleRow>,
    /// True when the loaded definition carried a rule kind this build does not
    /// understand. Saving would silently drop it, so the editor refuses.
    has_unknown_rule: bool,
}

impl EditorState {
    fn new() -> Self {
        Self {
            id: None,
            name: String::new(),
            slug: String::new(),
            description: String::new(),
            contact_info: String::new(),
            kb_article_id: String::new(),
            is_active: true,
            // A definition needs at least one field to be creatable, so the
            // editor opens with an empty row rather than an empty list the
            // operator has to discover the "Add field" button to escape.
            fields: vec![FieldRow::new()],
            rules: Vec::new(),
            has_unknown_rule: false,
        }
    }

    fn from_existing(def: &FormDefinition) -> Self {
        let mut has_unknown_rule = false;
        let rules = def
            .rules
            .iter()
            .filter_map(|r| match r {
                FormRule::RequiredIf {
                    field,
                    when_field,
                    equals,
                } => Some(RuleRow {
                    field: field.clone(),
                    when_field: when_field.clone(),
                    equals: equals.clone(),
                }),
                FormRule::Other => {
                    has_unknown_rule = true;
                    None
                }
            })
            .collect();

        Self {
            id: Some(def.id.to_string()),
            name: def.name.clone(),
            slug: def.slug.clone(),
            description: def.description.clone().unwrap_or_default(),
            contact_info: def.contact_info.clone().unwrap_or_default(),
            kb_article_id: def.kb_article_id.map(|i| i.to_string()).unwrap_or_default(),
            is_active: def.is_active,
            fields: def
                .fields
                .iter()
                .map(|f| FieldRow {
                    name: f.name.clone(),
                    label: f.label.clone(),
                    help_text: f.help_text.clone().unwrap_or_default(),
                    field_type: FieldType::from_str(&f.field_type).unwrap_or(FieldType::Text),
                    is_required: f.is_required,
                    max_length: f.max_length.map(|m| m.to_string()).unwrap_or_default(),
                    options: f.options.clone().unwrap_or_default().join(", "),
                    date_not_in_past: f.date_not_in_past,
                    // The key answers are already stored under. Never derived.
                    name_touched: true,
                })
                .collect(),
            rules,
            has_unknown_rule,
        }
    }
}

/// Derive a slug from the form name, so the operator does not have to think
/// about the link-stable identifier. Mirrors the server's `validate_slug`
/// shape (lowercase alphanumerics, single hyphens, no leading or trailing
/// hyphen) rather than approximating it.
fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_hyphen = true; // suppresses a leading hyphen
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_hyphen = false;
        } else if !last_hyphen {
            out.push('-');
            last_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

// ============================================================================
// PMS-754: DRAFTS
// ============================================================================

/// Where a half-built definition is kept between visits.
///
/// Keyed by definition id, with a distinct key for a new form, so editing two
/// definitions cannot cross-contaminate. Per browser and per device, which is
/// the right scope for "I was typing this a minute ago": `form_definitions`
/// requires a name, a slug and at least one field, so a half-built definition
/// is not a row the server would accept, and making it one would mean a
/// nullable-everything draft state and a decision about whose drafts other
/// operators can see. That is a feature; this is a data-loss fix.
/// PMS-759: how long typing has to stop before the draft is pushed to the
/// server. Short enough that a tab closed mid-thought loses at most half a
/// second of work on top of what the instant local write already holds, long
/// enough that a sentence typed at speed is one request rather than forty.
const DRAFT_DEBOUNCE_MS: u32 = 500;

fn draft_key(id: Option<&str>) -> String {
    match id {
        Some(id) => format!("mokosh.form-draft.{id}"),
        None => "mokosh.form-draft.new".to_string(),
    }
}

/// The persisted shape of an in-progress definition.
///
/// Deliberately its own type rather than serde on [`EditorState`]. The stored
/// JSON outlives the build that wrote it, so the editor's internals are free to
/// change without a stale draft failing to parse; `field_type` is a string here
/// for the same reason, and because [`FieldType`] carries no serde derives.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct DraftForm {
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    contact_info: String,
    #[serde(default)]
    kb_article_id: String,
    #[serde(default)]
    is_active: bool,
    #[serde(default)]
    fields: Vec<DraftField>,
    #[serde(default)]
    rules: Vec<DraftRule>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct DraftField {
    #[serde(default)]
    name: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    help_text: String,
    #[serde(default)]
    field_type: String,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    max_length: String,
    #[serde(default)]
    options: String,
    #[serde(default)]
    date_not_in_past: bool,
    #[serde(default)]
    name_touched: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct DraftRule {
    #[serde(default)]
    field: String,
    #[serde(default)]
    when_field: String,
    #[serde(default)]
    equals: String,
}

impl DraftForm {
    fn from_state(state: &EditorState) -> Self {
        Self {
            name: state.name.clone(),
            slug: state.slug.clone(),
            description: state.description.clone(),
            contact_info: state.contact_info.clone(),
            kb_article_id: state.kb_article_id.clone(),
            is_active: state.is_active,
            fields: state
                .fields
                .iter()
                .map(|f| DraftField {
                    name: f.name.clone(),
                    label: f.label.clone(),
                    help_text: f.help_text.clone(),
                    field_type: f.field_type.as_str().to_string(),
                    is_required: f.is_required,
                    max_length: f.max_length.clone(),
                    options: f.options.clone(),
                    date_not_in_past: f.date_not_in_past,
                    name_touched: f.name_touched,
                })
                .collect(),
            rules: state
                .rules
                .iter()
                .map(|r| DraftRule {
                    field: r.field.clone(),
                    when_field: r.when_field.clone(),
                    equals: r.equals.clone(),
                })
                .collect(),
        }
    }

    /// Rebuild an editor state, taking identity from `base`.
    ///
    /// `id` and `has_unknown_rule` come from the definition as the server sent
    /// it, never from storage: a draft must not be able to retarget which
    /// definition is being edited, nor to clear the flag that blocks saving a
    /// rule this build cannot represent.
    fn into_state(self, base: &EditorState) -> EditorState {
        EditorState {
            id: base.id.clone(),
            name: self.name,
            slug: self.slug,
            description: self.description,
            contact_info: self.contact_info,
            kb_article_id: self.kb_article_id,
            is_active: self.is_active,
            fields: self
                .fields
                .into_iter()
                .map(|f| FieldRow {
                    name: f.name,
                    label: f.label,
                    help_text: f.help_text,
                    field_type: FieldType::from_str(&f.field_type).unwrap_or(FieldType::Text),
                    is_required: f.is_required,
                    max_length: f.max_length,
                    options: f.options,
                    date_not_in_past: f.date_not_in_past,
                    name_touched: f.name_touched,
                })
                .collect(),
            rules: self
                .rules
                .into_iter()
                .map(|r| RuleRow {
                    field: r.field,
                    when_field: r.when_field,
                    equals: r.equals,
                })
                .collect(),
            has_unknown_rule: base.has_unknown_rule,
        }
    }
}

/// PMS-759: the local draft plus when it was written, or `None`.
///
/// The timestamp is what makes "the newer copy wins" decidable when the server
/// also holds a draft for this form. A draft written by a build before this one
/// is a bare [`DraftForm`] with no envelope; it parses, and reads as
/// arbitrarily old, so the server copy wins. That is the right way round: a
/// legacy local draft is by definition from before server drafts existed.
fn load_local_draft(id: Option<&str>) -> Option<(DraftForm, f64)> {
    let raw = crate::utils::prefs::get_str(&draft_key(id), "");
    if raw.is_empty() {
        return None;
    }
    if let Ok(stored) = serde_json::from_str::<StoredDraft>(&raw) {
        return Some((stored.form, stored.saved_at));
    }
    serde_json::from_str::<DraftForm>(&raw)
        .ok()
        .map(|d| (d, 0.0))
}

/// Write the local draft, stamped. The instant tier: a closed tab, a route
/// change and a crash do not wait for the 500 ms the server write is debounced
/// by, and a network write cannot be made synchronous on unload (`sendBeacon`
/// cannot carry the bearer token this SPA authenticates with).
fn store_local_draft(id: Option<&str>, form: &DraftForm) {
    let stored = StoredDraft {
        saved_at: now_ms(),
        form: form.clone(),
    };
    if let Ok(json) = serde_json::to_string(&stored) {
        crate::utils::prefs::set_str(&draft_key(id), &json);
    }
}

/// Milliseconds since the epoch, for stamping a local draft. Only ever
/// compared against another stamp from the same clock or against a server
/// timestamp, so clock skew shifts which copy wins by the size of the skew and
/// nothing worse.
fn now_ms() -> f64 {
    #[cfg(feature = "web")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(feature = "web"))]
    {
        0.0
    }
}

/// A draft the server holds, as `GET /forms/drafts` returns it.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
struct ServerDraft {
    id: String,
    #[serde(default)]
    form_definition_id: Option<String>,
    /// `payload -> name`, lifted out by the server so the drafts list has a
    /// label without parsing the snapshot.
    #[serde(default)]
    name: Option<String>,
    payload: DraftForm,
    /// RFC 3339, as Postgres wrote it.
    updated_at: String,
}

impl ServerDraft {
    /// The same milliseconds-since-epoch scale as [`now_ms`], so the two tiers
    /// are comparable. An unparseable timestamp reads as arbitrarily old,
    /// which resolves to "keep what is in this browser" rather than silently
    /// replacing local work with a copy of unknown age.
    fn saved_at(&self) -> f64 {
        chrono::DateTime::parse_from_rfc3339(&self.updated_at)
            .map(|t| t.timestamp_millis() as f64)
            .unwrap_or(0.0)
    }

    /// What to show in the drafts list. A draft is normally started before it
    /// is named, so the untitled case is the common one rather than an edge.
    fn label(&self) -> String {
        match self
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        {
            Some(n) => n.to_string(),
            None => "Untitled form".to_string(),
        }
    }
}

/// PMS-759: the local envelope. The bare [`DraftForm`] is still what is stored
/// inside it, so the shape that outlives a build is unchanged and only gains a
/// timestamp around it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct StoredDraft {
    #[serde(default)]
    saved_at: f64,
    form: DraftForm,
}

/// Pick the copy to restore, given what this browser holds and what the server
/// holds.
///
/// Newest wins. Neither tier is authoritative: the local one is written
/// instantly and so is usually ahead on the machine that typed it, and the
/// server one is the only thing that exists on any other machine.
fn newest_draft(
    local: Option<(DraftForm, f64)>,
    server: Option<&ServerDraft>,
) -> Option<DraftForm> {
    match (local, server) {
        (Some((form, at)), Some(remote)) => {
            if remote.saved_at() > at {
                Some(remote.payload.clone())
            } else {
                Some(form)
            }
        }
        (Some((form, _)), None) => Some(form),
        (None, Some(remote)) => Some(remote.payload.clone()),
        (None, None) => None,
    }
}

/// The organisation clients see on anything this page sends, or an empty string
/// until it loads. Empty suppresses the attribution rather than previewing a
/// form "sent to you by " nobody.
///
/// PMS-752: read from mokosh, NOT from `active_org_name()`. That one reads the
/// org switcher, which reads bunyip's `/v1/auth/memberships`, which 401s and
/// falls back to a synthetic membership whose name is the user's EMAIL ADDRESS
/// (MAPPS-427). So the preview showed an operator "This form was sent to you by
/// long@example.com" while the email said "Niceguy IT". `/tenants/current` is
/// the column the email is actually composed from.
fn use_org_identity() -> (String, Option<String>) {
    #[derive(Clone, Debug, PartialEq, serde::Deserialize)]
    struct TenantView {
        #[serde(default)]
        name: String,
        #[serde(default)]
        branding: BrandingView,
    }

    /// MAPPS-429: only the logo is read here; the rest of `TenantBranding` has
    /// nothing to do with previewing a form.
    #[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
    struct BrandingView {
        #[serde(default)]
        logo_url: Option<String>,
    }

    let tenant = use_resource(|| async {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        crate::hooks::fetch::api::get_authed::<TenantView>("/tenants/current")
            .await
            .ok()
    });
    match &*tenant.read_unchecked() {
        Some(Some(t)) => (t.name.clone(), t.branding.logo_url.clone()),
        _ => (String::new(), None),
    }
}

/// PMS-747: derive a field's reference name from its label, so "Phone number"
/// gives `phone_number` and the operator never has to invent a payload key.
///
/// The form's own link name has followed the form name since PMS-731
/// ([`slugify`]); fields were left typing theirs by hand. Same idea, different
/// alphabet: a reference name is an identifier, so it separates with
/// underscores rather than hyphens and must open with a letter.
///
/// A label starting with a digit ("2nd contact") is prefixed rather than
/// trimmed. Dropping the digit would produce `nd_contact`, which no longer
/// reads like the label it came from.
fn field_name_from_label(label: &str) -> String {
    let mut out = String::new();
    let mut last_underscore = true; // suppresses a leading underscore
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert_str(0, "f_");
    }
    out
}

/// PMS-747: why the server would reject this reference name, if it would.
///
/// Mirrors `validate_field_name` in mokosh-server (`src/modules/forms/models.rs`)
/// rather than approximating it. Without this a hand-edited "Last Name" is only
/// refused after a round trip, by a 400 that lands in the banner at the top of a
/// modal the operator has scrolled past.
fn field_name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("A reference name is required.");
    }
    let shape_ok = name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !name.ends_with('_')
        && !name.contains("__");
    (!shape_ok)
        .then_some("Lowercase letters, digits and single underscores, starting with a letter.")
}

/// PMS-747: what is wrong with one field row, in the slots that can show it.
///
/// Row problems used to exist only as a sentence in the form-level banner,
/// which sits at the top of the one region the modal scrolls. Carrying them
/// per row lets the offending input be marked where the operator is looking.
#[derive(Clone, Debug, Default, PartialEq)]
struct FieldRowErrors {
    label: String,
    name: String,
    options: String,
}

impl FieldRowErrors {
    /// Whether this row has anything wrong with it. PMS-760: a collapsed row
    /// has no inputs on screen to carry a message, so the summary has to say
    /// that there is one.
    fn any(&self) -> bool {
        *self != Self::default()
    }
}

// ============================================================================
// PMS-760: SECTIONS
// ============================================================================

/// Which part of the definition the editor is showing.
///
/// The modal used to render all three in one scroll: six form-level settings
/// with a help line each, then every field fully expanded, then the rules. Only
/// one of the three is ever being worked on, so only one is shown. Tabs rather
/// than a wizard because editing an existing form is not a sequence: an
/// operator opens it to change one field and must not be walked through
/// Details to reach it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorSection {
    Details,
    Fields,
    Rules,
}

/// How many problems the last save attempt found, per section.
///
/// PMS-747 put a count in the pinned footer because the footer is the only
/// thing certain to be on screen when Create is pressed. With the sections
/// split, a bare total would point at work the operator cannot see, so the
/// count is kept per section: the tab bar shows where the problems are, and
/// [`ProblemCounts::first_section`] decides which tab a failed save lands on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProblemCounts {
    details: usize,
    fields: usize,
    rules: usize,
}

impl ProblemCounts {
    fn total(&self) -> usize {
        self.details + self.fields + self.rules
    }

    /// The section to show after a failed save: the first one, in the order
    /// they are presented, that has anything wrong with it. `None` when the
    /// save had no problems, in which case the operator stays where they are.
    fn first_section(&self) -> Option<EditorSection> {
        if self.details > 0 {
            Some(EditorSection::Details)
        } else if self.fields > 0 {
            Some(EditorSection::Fields)
        } else if self.rules > 0 {
            Some(EditorSection::Rules)
        } else {
            None
        }
    }
}

/// The set of expanded field rows after two rows swap places.
///
/// Expansion is keyed by position because a [`FieldRow`] has no identity of its
/// own: the payload key is editable, and the draft round-trip would not carry a
/// synthetic id. Position keys mean the structural operations have to move the
/// expansion with the row, or moving an open field up would leave the field it
/// displaced showing as open instead.
fn expansion_after_swap(expanded: &HashSet<usize>, a: usize, b: usize) -> HashSet<usize> {
    expanded
        .iter()
        .map(|&i| {
            if i == a {
                b
            } else if i == b {
                a
            } else {
                i
            }
        })
        .collect()
}

/// The set of expanded field rows after one row is removed. Rows after it shift
/// down by one; the removed row's own entry goes with it.
fn expansion_after_remove(expanded: &HashSet<usize>, removed: usize) -> HashSet<usize> {
    expanded
        .iter()
        .filter(|&&i| i != removed)
        .map(|&i| if i > removed { i - 1 } else { i })
        .collect()
}

/// Move one item to another position, closing the gap behind it.
///
/// What a drag does, as against the swap the arrow buttons do: dropping field 5
/// onto field 1 puts it at 1 and pushes 1..4 down, rather than exchanging the
/// two and scrambling everything between them. Out-of-range indices and a move
/// to where the item already is are no-ops, so a drag that ends on itself
/// leaves the list alone.
fn move_row<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from == to || from >= items.len() || to >= items.len() {
        return;
    }
    let item = items.remove(from);
    items.insert(to, item);
}

/// The set of expanded field rows after one row is dragged to a new position.
///
/// The counterpart of [`move_row`] for expansion state, which is keyed by
/// position. Rows between the two ends shift by one, in whichever direction
/// closes the gap; the dragged row's own entry lands on its new index.
fn expansion_after_move(expanded: &HashSet<usize>, from: usize, to: usize) -> HashSet<usize> {
    if from == to {
        return expanded.clone();
    }
    expanded
        .iter()
        .map(|&i| {
            if i == from {
                to
            } else if from < to && i > from && i <= to {
                i - 1
            } else if to < from && i >= to && i < from {
                i + 1
            } else {
                i
            }
        })
        .collect()
}

/// The set of expanded field rows after a new field is added.
///
/// MAPPS-434: exactly the new row. Opening it is right, because it has nothing
/// to summarise yet and typing into it is the whole reason it was added, but
/// leaving the previous one open behind it rebuilt the wall PMS-760 removed:
/// five clicks of Add field left five editors on screen, each with its label,
/// type, hint, required checkbox and Advanced disclosure.
///
/// Only this path collapses anything. Clicking a row's own summary still opens
/// it alongside whatever else is open, because comparing two fields is a real
/// thing to want, and a failed save still opens every row that has a problem,
/// which can be several at once.
fn expansion_after_add(new_index: usize) -> HashSet<usize> {
    HashSet::from([new_index])
}

/// What a collapsed field row calls itself.
///
/// The label is what the client reads, so it is what the operator recognises
/// the row by. A field being typed has neither label nor reference name yet,
/// and a blank summary row is unclickable-looking, so it says so.
fn field_summary_label(row: &FieldRow) -> String {
    let label = row.label.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    let name = row.name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    "Untitled field".to_string()
}

// ============================================================================
// EDITOR MODAL
// ============================================================================

#[component]
fn FormEditorModal(
    state: EditorState,
    /// PMS-759: the server's draft for this form, if it holds one. Passed in
    /// rather than fetched here: the page has already loaded the list for its
    /// own Drafts section, and a second request would race the restore
    /// decision, which has to be made once, at open time.
    server_draft: Option<ServerDraft>,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
) -> Element {
    let is_edit = state.id.is_some();
    let save_id = state.id.clone();
    let draft_key = draft_key(state.id.as_deref());

    // PMS-754: a draft left by an earlier visit, if it differs from what the
    // server holds. Read once on mount; `use_hook` rather than a signal because
    // restoring is a decision made at open time, not a value that changes.
    let saved = use_hook({
        let state = state.clone();
        move || state
    });
    let restored = use_hook({
        let saved = saved.clone();
        let server_draft = server_draft.clone();
        move || {
            // PMS-759: whichever tier is newer. On a second machine only the
            // server has anything; on the machine that typed it, the local
            // copy is usually ahead because it is written on every change
            // rather than on a debounce.
            let picked =
                newest_draft(load_local_draft(saved.id.as_deref()), server_draft.as_ref())?;
            let restored = picked.into_state(&saved);
            // A draft equal to the saved definition is not worth announcing:
            // it would restore nothing and only tell the operator their work
            // was at risk when it was not.
            (restored != saved).then_some(restored)
        }
    });
    let mut draft_restored = use_signal(|| restored.is_some());
    let state = restored.unwrap_or_else(|| saved.clone());

    let mut name = use_signal(|| state.name.clone());
    let mut slug = use_signal(|| state.slug.clone());
    let mut slug_touched = use_signal(|| is_edit);
    let mut description = use_signal(|| state.description.clone());
    let mut contact_info = use_signal(|| state.contact_info.clone());
    let mut kb_article_id = use_signal(|| state.kb_article_id.clone());
    let mut is_active = use_signal(|| state.is_active);
    let mut fields = use_signal(|| state.fields.clone());
    let mut rules = use_signal(|| state.rules.clone());
    let has_unknown_rule = state.has_unknown_rule;

    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut name_error = use_signal(String::new);
    let mut slug_error = use_signal(String::new);
    // PMS-747: per-row problems, indexed alongside `fields`, plus the count the
    // pinned footer reports. The footer is the only place guaranteed to be on
    // screen when Create is pressed.
    let mut field_errors = use_signal(Vec::<FieldRowErrors>::new);
    // PMS-760: the same count, split by section so the tab bar can say where
    // the problems are rather than only how many there are.
    let mut problems = use_signal(ProblemCounts::default);
    // PMS-760: which section is showing, and which field rows are open.
    // Details first: it is where a new form starts, and where the two required
    // values live. A single-field form opens with that field expanded, because
    // collapsing the only row would show an operator nothing at all.
    let mut section = use_signal(|| EditorSection::Details);
    let mut expanded_fields = use_signal(|| {
        if state.fields.len() == 1 {
            HashSet::from([0usize])
        } else {
            HashSet::new()
        }
    });
    // PMS-760: the drag in progress. `drag_from` is the row that was picked up
    // and `drag_over` the row it is currently above; both live here rather than
    // in the row, because a drag is a conversation between two of them.
    let drag_from = use_signal(|| None::<usize>);
    let drag_over = use_signal(|| None::<usize>);
    // PMS-744: preview of the client's view, built from the live editor state.
    let mut previewing = use_signal(|| false);
    // PMS-754: asked before a close that would throw work away.
    let mut confirming_discard = use_signal(|| false);

    // The editor as it stands, rebuilt from the signals. One value to compare
    // against the saved definition and one to persist, so "is this dirty" and
    // "what would be restored" can never disagree.
    let current = EditorState {
        id: save_id.clone(),
        name: name(),
        slug: slug(),
        description: description(),
        contact_info: contact_info(),
        kb_article_id: kb_article_id(),
        is_active: is_active(),
        fields: fields(),
        rules: rules(),
        has_unknown_rule,
    };
    let dirty = current != saved;

    // PMS-754: autosave. Writing on every change rather than on an interval,
    // because the failures this exists for (a closed tab, a route change, a
    // crash) do not wait for a timer.
    // PMS-759: and the server half, debounced, so the draft survives this
    // machine as well as this tab. `server_draft_id` is what the discard path
    // deletes; it starts as whatever the page found and is replaced by each
    // successful write, because the first write on a new form is what creates
    // the row.
    let mut server_draft_id = use_signal(|| server_draft.as_ref().map(|d| d.id.clone()));
    // The most recent snapshot, so a debounced write that wakes up superseded
    // can tell. Reading the editor signals directly from the task would work
    // too and would mean rebuilding the whole `EditorState` inside it.
    let mut latest_snapshot = use_signal(DraftForm::default);
    {
        let key = draft_key.clone();
        let snapshot = DraftForm::from_state(&current);
        let baseline = DraftForm::from_state(&saved);
        let definition_id = save_id.clone();
        use_effect(move || {
            if snapshot == baseline {
                // Back to what the server holds: nothing to restore, and a
                // stored copy would only produce a "restored" banner that
                // changes nothing on the next open. The server row is left
                // alone rather than deleted, because a delete on every
                // keystroke that happens to match the saved state would race
                // the writes around it; saving the form is what retires it.
                crate::utils::prefs::clear(&key);
                return;
            }
            store_local_draft(definition_id.as_deref(), &snapshot);
            latest_snapshot.set(snapshot.clone());

            let snapshot = snapshot.clone();
            let definition_id = definition_id.clone();
            spawn(async move {
                #[cfg(feature = "web")]
                {
                    // The debounce. Every change spawns one of these; the ones
                    // whose snapshot has been superseded by the time they wake
                    // up drop out, so a burst of typing costs one request.
                    gloo_timers::future::TimeoutFuture::new(DRAFT_DEBOUNCE_MS).await;
                    if latest_snapshot() != snapshot {
                        return;
                    }
                    let body = serde_json::json!({
                        "form_definition_id": definition_id,
                        "payload": snapshot,
                    });
                    match crate::hooks::fetch::api::put_authed_typed::<ServerDraft, _>(
                        "/forms/drafts",
                        &body,
                    )
                    .await
                    {
                        Ok(saved) => server_draft_id.set(Some(saved.id)),
                        // Deliberately silent. The local copy has already been
                        // written, so a failed autosave costs the cross-device
                        // half and nothing the operator can act on; a toast on
                        // every keystroke of an offline session would be worse
                        // than the thing it reports.
                        Err(e) => tracing::debug!("form draft autosave failed: {e:?}"),
                    }
                }
                #[cfg(not(feature = "web"))]
                {
                    let _ = (&snapshot, &definition_id);
                }
            });
        });
    }

    // MAPPS-292: the browser half. Covers closing the tab and reloading, which
    // no in-app confirmation can reach. This page never opted in, which is why
    // a half-built definition died silently on a tab close.
    crate::hooks::use_unsaved_guard(use_memo(move || dirty).into());
    let draft_key_for_close = draft_key.clone();
    let draft_key_for_discard = draft_key.clone();
    let saved_for_discard = saved.clone();
    // MAPPS-436: Discard draft throws away restored work and deletes the
    // server-side copy, so it opens the dialog and mutates only on confirm.
    let mut confirming_draft_discard = use_signal(|| false);
    let discard_restored_draft = move |_: ()| {
        confirming_draft_discard.set(false);
        let base = saved_for_discard.clone();
        name.set(base.name.clone());
        slug.set(base.slug.clone());
        description.set(base.description.clone());
        contact_info.set(base.contact_info.clone());
        kb_article_id.set(base.kb_article_id.clone());
        is_active.set(base.is_active);
        fields.set(base.fields.clone());
        rules.set(base.rules.clone());
        field_errors.set(Vec::new());
        problems.set(ProblemCounts::default());
        crate::utils::prefs::clear(&draft_key_for_discard);
        draft_restored.set(false);
        // PMS-759: and the server's copy, or the next open on any machine
        // restores exactly what was just discarded.
        if let Some(id) = server_draft_id() {
            server_draft_id.set(None);
            spawn(async move {
                if let Err(err) =
                    crate::hooks::fetch::api::delete_authed(&format!("/forms/drafts/{id}")).await
                {
                    // The local state is already reverted; say so rather than
                    // leave a server draft that reappears on the next open.
                    crate::hooks::push_toast(
                        crate::components::AlertType::Error,
                        format!("Could not discard the saved draft: {err}"),
                    );
                }
            });
        }
    };
    // PMS-748: see `FormsBuilderPage`; the preview must name the MSP the
    // client's own page will name.
    let (tenant_name, tenant_logo) = use_org_identity();

    // Published articles for the procedure picker.
    let articles = use_resource(|| async {
        let _token = crate::hooks::fetch::api::current_access_token()?;
        crate::hooks::fetch::api::get_authed_typed::<Paginated<KbArticle>>(&format!(
            "/kb/articles?page=1&per_page={ARTICLE_PAGE_SIZE}"
        ))
        .await
        .ok()
    });
    let article_options: Vec<SelectOption> = match &*articles.read_unchecked() {
        Some(Some(page)) => page
            .data
            .iter()
            .map(|a| SelectOption::new(a.id.to_string(), a.title.clone()))
            .collect(),
        _ => Vec::new(),
    };

    // PMS-756: a callback rather than a closure, because the close prompt
    // needs to run the SAME save the footer button runs. Two copies of this
    // logic is how the two paths would come to validate differently.
    let handle_save = use_callback(move |_: ()| {
        if saving() {
            return;
        }

        // docs/form-conventions.md: evaluate every required field, set each
        // slot, then bail once. Field-row problems have no inline slot of
        // their own, so they land in the form-level banner naming the row.
        name_error.set(String::new());
        slug_error.set(String::new());
        error.set(String::new());
        let mut failed = false;

        if name.read().trim().is_empty() {
            name_error.set("Name is required.".to_string());
            failed = true;
        }
        if slug.read().trim().is_empty() {
            slug_error.set("Link name is required.".to_string());
            failed = true;
        }

        let rows = fields.read().clone();
        // PMS-760: field problems and rule problems are collected apart, so
        // the tab bar can point at the section holding each. They are still
        // joined into one banner below, because the banner sits above the tabs
        // and is the only surface that can speak for a section you are not on.
        let mut row_problems: Vec<String> = Vec::new();
        // PMS-747: every problem is written twice on purpose. Once into the row
        // it belongs to, so the input is marked where the operator is working,
        // and once into the banner, which is the only surface that can say
        // anything about a row scrolled out of view.
        let mut row_errors = vec![FieldRowErrors::default(); rows.len()];
        if rows.is_empty() {
            row_problems.push("A form needs at least one field.".to_string());
        }
        for (i, f) in rows.iter().enumerate() {
            let n = i + 1;
            if let Some(problem) = field_name_problem(f.name.trim()) {
                row_errors[i].name = problem.to_string();
                row_problems.push(format!("Field {n}: {problem}"));
            }
            if f.label.trim().is_empty() {
                row_errors[i].label = "A label is required.".to_string();
                row_problems.push(format!("Field {n} needs a label."));
            }
            let duplicate_of = (!f.name.trim().is_empty())
                .then(|| {
                    rows.iter()
                        .take(i)
                        .position(|prior| prior.name.trim() == f.name.trim())
                })
                .flatten();
            if let Some(prior) = duplicate_of {
                row_errors[i].name = format!("Already used by field {}.", prior + 1);
                row_problems.push(format!(
                    "Field {n} repeats the reference name `{}`.",
                    f.name.trim()
                ));
            }
            // The server rejects a select with no options at write time; catch
            // it here so the operator is not told about it after a round trip.
            if f.field_type.needs_options() && f.parsed_options().is_empty() {
                row_errors[i].options = "At least one choice is required.".to_string();
                row_problems.push(format!(
                    "Field {n} is a choice list and needs at least one option."
                ));
            }
        }
        // PMS-760: a collapsed row cannot show its own error, so every row
        // that has one is opened. Added to what is already open rather than
        // replacing it: an operator who expanded a row to work on it should
        // find it still open after a failed save.
        let failing_rows: Vec<usize> = row_errors
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.any().then_some(i))
            .collect();
        field_errors.set(row_errors);
        if !failing_rows.is_empty() {
            expanded_fields.write().extend(failing_rows);
        }

        // A rule pointing at a field that no longer exists can never fire, and
        // the server rejects it, so it is caught here for the same reason.
        let rule_rows = rules.read().clone();
        let mut rule_problems: Vec<String> = Vec::new();
        for (i, r) in rule_rows.iter().enumerate() {
            let n = i + 1;
            if r.field.trim().is_empty() || r.when_field.trim().is_empty() {
                rule_problems.push(format!("Rule {n} needs both fields chosen."));
            }
            if r.equals.trim().is_empty() {
                rule_problems.push(format!("Rule {n} needs a value to match."));
            }
        }

        if has_unknown_rule {
            rule_problems.push(
                "This form has a rule this version cannot edit. Saving would remove it, so it is blocked. Update the app first.".to_string(),
            );
        }

        if !row_problems.is_empty() || !rule_problems.is_empty() {
            let mut all = row_problems.clone();
            all.extend(rule_problems.iter().cloned());
            error.set(all.join(" "));
            failed = true;
        }
        // Counts the top-level slots too, so "1 problem" cannot mean an empty
        // Name field the operator was never pointed at.
        let counts = ProblemCounts {
            details: usize::from(!name_error.read().is_empty())
                + usize::from(!slug_error.read().is_empty()),
            fields: row_problems.len(),
            rules: rule_problems.len(),
        };
        problems.set(counts);
        // PMS-760: and land on the section holding the first of them. Without
        // this the footer could report problems while the operator sits on a
        // tab where everything is fine.
        if let Some(target) = counts.first_section() {
            section.set(target);
        }
        if failed {
            return;
        }

        saving.set(true);

        let upsert_fields: Vec<UpsertFormField> = rows
            .iter()
            .enumerate()
            .map(|(i, f)| UpsertFormField {
                name: f.name.trim().to_string(),
                label: f.label.trim().to_string(),
                help_text: optional(&f.help_text),
                field_type: f.field_type.as_str().to_string(),
                is_required: f.is_required,
                min_length: None,
                max_length: f
                    .field_type
                    .honours_length()
                    .then(|| f.max_length.trim().parse::<i32>().ok())
                    .flatten(),
                options: f.field_type.needs_options().then(|| f.parsed_options()),
                date_not_in_past: matches!(f.field_type, FieldType::Date) && f.date_not_in_past,
                // Position IS the order, so the operator reorders by moving
                // rows rather than by typing numbers that can collide.
                sort_order: i as i32,
            })
            .collect();
        let upsert_rules: Vec<FormRule> = rule_rows
            .iter()
            .map(|r| FormRule::RequiredIf {
                field: r.field.trim().to_string(),
                when_field: r.when_field.trim().to_string(),
                equals: r.equals.trim().to_string(),
            })
            .collect();

        let draft_key_for_save = draft_key.clone();
        let article = uuid::Uuid::parse_str(kb_article_id.read().trim()).ok();
        let name_v = name.read().trim().to_string();
        let slug_v = slug.read().trim().to_string();
        let desc_v = optional(&description.read());
        let contact_v = optional(&contact_info.read());
        let active_v = *is_active.read();
        let id = save_id.clone();

        spawn(async move {
            #[cfg(feature = "web")]
            {
                let result = match id {
                    None => {
                        let req = CreateFormDefinitionRequest {
                            name: name_v,
                            slug: slug_v,
                            description: desc_v,
                            contact_info: contact_v,
                            kb_article_id: article,
                            rules: upsert_rules,
                            is_active: active_v,
                            fields: upsert_fields,
                        };
                        crate::hooks::fetch::api::post_authed_typed::<FormDefinition, _>(
                            "/forms", &req,
                        )
                        .await
                        .map(|_| ())
                    }
                    Some(id) => {
                        let req = UpdateFormDefinitionRequest {
                            name: name_v,
                            description: desc_v,
                            contact_info: contact_v,
                            kb_article_id: article,
                            rules: upsert_rules,
                            is_active: active_v,
                            fields: upsert_fields,
                        };
                        crate::hooks::fetch::api::patch_authed_typed::<FormDefinition, _>(
                            &format!("/forms/{id}"),
                            &req,
                        )
                        .await
                        .map(|_| ())
                    }
                };
                match result {
                    Ok(()) => {
                        // PMS-754: the work is on the server now, so the draft
                        // has nothing left to protect and the next New Form
                        // must start empty. PMS-759: only the LOCAL copy is
                        // dropped here. The server retires its own on create
                        // and update, because a draft exists to survive the
                        // browser going away and so cannot depend on the
                        // browser to tidy up.
                        crate::utils::prefs::clear(&draft_key_for_save);
                        // MAPPS-424: naming the next step here, because saving
                        // is the moment the user expects a link and does not
                        // get one. The form is defined; nothing has been sent.
                        crate::hooks::push_toast(
                            crate::components::AlertType::Success,
                            "Request form saved. Use Send on its row to email a client a link to it.",
                        );
                        onsaved.call(());
                    }
                    Err(err) => {
                        // The server's own per-field errors are keyed
                        // `fields[3].options` / `rules[0].field`, which have no
                        // inline slot here, so they go to the banner where they
                        // stay readable rather than being dropped.
                        crate::hooks::push_api_error(&err);
                        error.set(err.user_message());
                    }
                }
            }
            saving.set(false);
        });
    });

    // PMS-754: every way out of this modal lands here. `Modal` routes the X,
    // Esc and a backdrop click to one `onclose`, and the backdrop is the
    // largest click target on the screen, so it is the one that gets hit by
    // accident. An untouched form still closes immediately: a confirmation on
    // work nobody did is how people learn to click through confirmations.
    let request_close = use_callback(move |_: ()| {
        if dirty {
            confirming_discard.set(true);
        } else {
            crate::utils::prefs::clear(&draft_key_for_close);
            onclose.call(());
        }
    });

    let can_mutate = crate::hooks::use_can_mutate();
    let footer = rsx! {
        // PMS-747: the footer is pinned while the body scrolls, so this is the
        // only place a failed Create is certain to be seen. Without it, pressing
        // Create at the bottom of a three-field form set an error banner several
        // hundred pixels above the viewport and looked like a dead button.
        if problems().total() > 0 {
            span {
                // PMS-760: this named a danger colour token input.css never
                // defined, so it emitted no CSS and read as ordinary text. The
                // red/green/yellow state scale is what the theme guard allows.
                class: "mr-auto self-center text-sm text-red-600 dark:text-red-400",
                role: "alert",
                if problems().total() == 1 {
                    "1 problem to fix above."
                } else {
                    "{problems().total()} problems to fix above."
                }
            }
        }
        // PMS-744: preview before committing. Sits with Cancel and Save
        // because that is the moment the question arises ("is this what they
        // will see?"), and it reads the CURRENT editor state, so it answers
        // for unsaved edits too.
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| previewing.set(true),
            "Preview"
        }
        Button { variant: ButtonVariant::Secondary, onclick: move |_| request_close.call(()), "Cancel" }
        Button {
            variant: ButtonVariant::Primary,
            loading: saving(),
            disabled: !can_mutate,
            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
            onclick: move |_| handle_save.call(()),
            if is_edit { "Save Changes" } else { "Create Form" }
        }
    };

    let field_names: Vec<SelectOption> = fields
        .read()
        .iter()
        .filter(|f| !f.name.trim().is_empty())
        .map(|f| {
            let n = f.name.trim().to_string();
            let label = if f.label.trim().is_empty() {
                n.clone()
            } else {
                f.label.trim().to_string()
            };
            SelectOption::new(n, label)
        })
        .collect();

    // PMS-760: the three parts of a definition, one at a time. PMS-763: handed
    // to the modal's pinned subheader rather than made sticky inside the body.
    // Sticky put it 16px down from the top of the scrollport, because a sticky
    // box is constrained by the scrollport inset by the scroll container's
    // padding, and the field list scrolled through the strip above it. Pinned
    // outside the scrolling region, there is no strip to bleed through.
    let section_tabs = rsx! {
        nav { class: "-mb-px flex gap-6", role: "tablist", aria_label: "Form sections",
            EditorTabButton {
                label: "Details",
                count: None,
                problems: problems().details,
                active: section() == EditorSection::Details,
                onclick: move |_| section.set(EditorSection::Details),
            }
            EditorTabButton {
                label: "Fields",
                count: Some(fields.read().len()),
                problems: problems().fields,
                active: section() == EditorSection::Fields,
                onclick: move |_| section.set(EditorSection::Fields),
            }
            EditorTabButton {
                label: "Rules",
                count: Some(rules.read().len()),
                problems: problems().rules,
                active: section() == EditorSection::Rules,
                onclick: move |_| section.set(EditorSection::Rules),
            }
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: if is_edit { "Edit request form".to_string() } else { "New request form".to_string() },
            size: crate::components::ModalSize::Large,
            onclose: move |_| request_close.call(()),
            footer,
            subheader: section_tabs,

            div { class: "space-y-4",

                // PMS-754: a restored draft says so. Silently repopulating a
                // form is its own surprise, and on an existing definition the
                // draft may be older than what the server now holds, so the
                // operator is told and offered the saved version back.
                if draft_restored() {
                    div { class: "rounded-md border border-line bg-surface-2 px-3 py-2 text-sm text-content flex items-center justify-between gap-3",
                        span {
                            "Restored what you had unsaved. It has not been saved to the server."
                        }
                        Button {
                            variant: ButtonVariant::Link,
                            onclick: move |_| confirming_draft_discard.set(true),
                            "Discard draft"
                        }
                    }
                }
                // MAPPS-436: the revert and the DELETE fire from `onconfirm` only.
                crate::components::ConfirmDialog {
                    open: confirming_draft_discard(),
                    title: "Discard draft".to_string(),
                    message: "Discard the restored draft and go back to the saved form? The unsaved changes are gone for good."
                        .to_string(),
                    confirm_text: "Discard".to_string(),
                    cancel_text: "Keep draft".to_string(),
                    destructive: true,
                    onconfirm: discard_restored_draft,
                    oncancel: move |_| confirming_draft_discard.set(false),
                }

                // Above the tab bar on purpose: it is the one surface that can
                // speak for a section the operator is not looking at, and it
                // names the rows and rules its messages belong to.
                if !error().is_empty() {
                    ErrorBanner { "{error()}" }
                }

                // --- definition ------------------------------------------------
                //
                // The inactive sections are hidden rather than unmounted, so a
                // half-typed field keeps its component-level state (PMS-516
                // validates an input on blur, and remounting would reset that)
                // and switching tabs costs no rebuild of the field list.
                div {
                    class: if section() == EditorSection::Details { "space-y-4" } else { "hidden" },
                    role: "tabpanel",
                    Input {
                        name: "name",
                        label: "Name",
                        value: name(),
                        required: true,
                        disabled: saving(),
                        error: name_error(),
                        help: "What the client sees at the top of the form.".to_string(),
                        oninput: move |e: FormEvent| {
                            let v = e.value();
                            // The slug follows the name until the operator
                            // edits it, then it stops moving: it is the
                            // link-stable identifier and links already sent
                            // must keep resolving.
                            if !slug_touched() {
                                slug.set(slugify(&v));
                            }
                            name.set(v);
                        },
                    }
                    Input {
                        name: "slug",
                        label: "Link name",
                        value: slug(),
                        required: true,
                        disabled: saving() || is_edit,
                        error: slug_error(),
                        help: if is_edit {
                            "Fixed after creation, because links already sent to clients use it.".to_string()
                        } else {
                            "Used in the link a client opens. Lowercase letters, numbers and hyphens.".to_string()
                        },
                        oninput: move |e: FormEvent| {
                            slug_touched.set(true);
                            slug.set(e.value());
                        },
                    }
                    Textarea {
                        name: "description",
                        label: "Description",
                        value: description(),
                        rows: 2,
                        disabled: saving(),
                        help: "Optional. Shown under the title on the client's form.".to_string(),
                        oninput: move |e: FormEvent| description.set(e.value()),
                    }
                    Input {
                        name: "contact_info",
                        label: "Contact for questions",
                        value: contact_info(),
                        disabled: saving(),
                        // PMS-748: optional because the MSP's NAME is shown to
                        // the client either way. This is the channel, not the
                        // attribution.
                        help: "Optional. Shown to the client on the form and in the email that links to it, so they can ask before they answer. Free text, e.g. \"the service desk on 555-0100\".".to_string(),
                        oninput: move |e: FormEvent| contact_info.set(e.value()),
                    }
                    Select {
                        name: "kb_article_id",
                        label: "Procedure article",
                        options: article_options,
                        value: kb_article_id(),
                        placeholder: "None".to_string(),
                        disabled: saving(),
                        help: "Attached to every ticket this form creates, so whoever works it has the procedure. Never shown to the client.".to_string(),
                        onchange: move |e: FormEvent| kb_article_id.set(e.value()),
                    }
                    Checkbox {
                        name: "is_active",
                        label: "Accepting submissions".to_string(),
                        checked: is_active(),
                        disabled: saving(),
                        help: "Turn off to retire the form. Existing submissions are kept; new ones are refused.".to_string(),
                        onchange: move |e: FormEvent| is_active.set(e.value() == "true" || e.value() == "on"),
                    }
                }

                // --- fields ----------------------------------------------------
                div {
                    class: if section() == EditorSection::Fields { "" } else { "hidden" },
                    role: "tabpanel",
                    // PMS-750: a plain heading, not a header row. The control
                    // that adds a field lives under the last row; see below.
                    // PMS-760: the heading itself is gone, because the tab is
                    // the heading. What is left is the one thing the list does
                    // not show by itself. The reference-name sentence went with
                    // it: that control now lives behind Advanced, and carries
                    // the same explanation as its own help text.
                    p { class: "text-xs text-muted mb-3",
                        "Order here is the order the client sees. Open a field to edit it, and drag a row by its handle to move it."
                    }

                    for (index, row) in fields.read().clone().into_iter().enumerate() {
                        FieldRowEditor {
                            key: "{index}",
                            index,
                            row,
                            total: fields.read().len(),
                            expanded: expanded_fields.read().contains(&index),
                            disabled: saving(),
                            errors: field_errors.read().get(index).cloned().unwrap_or_default(),
                            fields,
                            expanded_fields,
                            drag_from,
                            drag_over,
                        }
                    }

                    // PMS-747 put a control here because the section header is
                    // inside the only region the modal scrolls, so from the
                    // third field on the header copy was off-screen and adding
                    // a fourth meant scrolling back up past everything just
                    // typed. PMS-750: it is now the ONLY one. Two identical
                    // buttons asked the operator to choose between the same
                    // thing twice, and on a one-field form both were on screen
                    // at once.
                    //
                    // This is also where it belongs on its own merits: the end
                    // of the list is where the new row appears and where the
                    // cursor already is. Full width so it reads as part of the
                    // list, because the Conditional rules heading sits directly
                    // below and a small centred button between two sections
                    // could be read as belonging to either.
                    Button {
                        variant: ButtonVariant::Secondary,
                        class: "w-full".to_string(),
                        disabled: saving(),
                        onclick: move |_| {
                            // PMS-760: a new row opens, because it has nothing
                            // to summarise yet and typing into it is the whole
                            // reason it was added. MAPPS-434: and the row
                            // before it closes, or building a form one field at
                            // a time ends with every editor open.
                            let at = fields.read().len();
                            fields.write().push(FieldRow::new());
                            expanded_fields.set(expansion_after_add(at));
                        },
                        "+ Add field"
                    }
                }

                // --- rules -----------------------------------------------------
                div {
                    class: if section() == EditorSection::Rules { "" } else { "hidden" },
                    role: "tabpanel",
                    div { class: "flex items-center justify-end mb-2",
                        Button {
                            variant: ButtonVariant::Secondary,
                            disabled: saving() || field_names.len() < 2,
                            title: (field_names.len() < 2).then(|| "Name at least two fields first".to_string()),
                            onclick: move |_| rules.write().push(RuleRow {
                                field: String::new(),
                                when_field: String::new(),
                                equals: String::new(),
                            }),
                            "Add rule"
                        }
                    }
                    p { class: "text-xs text-muted mb-3",
                        "Make a field required only when another field has a particular answer. Everything else about a field is set on the field itself."
                    }

                    if rules.read().is_empty() {
                        p { class: "text-xs text-muted italic", "No conditional rules." }
                    }

                    for (index, rule) in rules.read().clone().into_iter().enumerate() {
                        RuleRowEditor {
                            key: "{index}",
                            index,
                            rule,
                            field_names: field_names.clone(),
                            disabled: saving(),
                            rules,
                        }
                    }
                }
            }
        }

        // PMS-754: the prompt for a close that would throw work away.
        //
        // PMS-756: three actions, not two, so it is a plain `Modal` rather than
        // the shared `ConfirmDialog`, which is confirm-or-cancel by
        // construction. Written inline because this is the only three-way
        // prompt in the product; widening the shared component for one caller
        // would make every other confirm carry the concept.
        if confirming_discard() {
            crate::components::Modal {
                open: true,
                title: "Save before closing?".to_string(),
                size: crate::components::ModalSize::Small,
                onclose: move |_| confirming_discard.set(false),
                footer: rsx! {
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| confirming_discard.set(false),
                        "Keep editing"
                    }
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| {
                            // The draft is deliberately KEPT: someone who
                            // closes and then realises they wanted the work
                            // back is the case a confirmation alone makes
                            // worse.
                            confirming_discard.set(false);
                            onclose.call(());
                        },
                        "Close without saving"
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        loading: saving(),
                        // Matching the footer button rather than offering an
                        // action that cannot work: a Save here while the same
                        // save is disabled below would fail silently or, worse,
                        // look like it worked.
                        disabled: !can_mutate,
                        title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
                        onclick: move |_| {
                            // Dismiss FIRST. A save from here runs the same
                            // validation as the footer button, and an abandoned
                            // half-built form is exactly the shape that fails
                            // it. Those errors land inline on the offending
                            // rows and as a count in the pinned footer
                            // (PMS-747), and this dialog is sitting on top of
                            // all of it.
                            confirming_discard.set(false);
                            handle_save.call(());
                        },
                        "Save and close"
                    }
                },

                p { class: "text-sm text-content",
                    "You have changes that have not been saved."
                }
                p { class: "mt-2 text-sm text-muted",
                    "Closing without saving keeps them here as a draft, so you can pick them up next time you open this form."
                }
            }
        }

        if previewing() {
            ClientPreviewModal {
                def: preview_form(
                    &name(),
                    &description(),
                    &contact_info(),
                    &tenant_name,
                    tenant_logo.as_deref(),
                    &fields.read(),
                    &rules.read(),
                ),
                unsaved: true,
                onclose: move |_| previewing.set(false),
            }
        }
    }
}

/// PMS-744: a SAVED definition as the client would receive it.
///
/// The sibling of [`preview_form`], for the list row, where there is no editor
/// state to read. Both feed the same modal and the same client component, so
/// the two entry points cannot disagree about what a client sees.
///
/// Fields are ordered by `sort_order` rather than trusted in arrival order:
/// the client is served them sorted, so a preview that showed them otherwise
/// would be wrong about the one thing the list column cannot tell you.
///
/// A rule this build cannot represent (`FormRule::Other`, authored by a newer
/// server) is dropped. It cannot be rendered honestly, and the editor already
/// refuses to save a definition carrying one.
fn preview_from_definition(
    def: &FormDefinition,
    tenant_name: &str,
    logo_url: Option<&str>,
) -> PublicForm {
    let mut fields: Vec<&crate::modules::forms::FormField> = def.fields.iter().collect();
    fields.sort_by_key(|f| f.sort_order);

    PublicForm {
        name: def.name.clone(),
        description: def
            .description
            .clone()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty()),
        // PMS-748: the attribution the client is served. The operator IS the
        // tenant here, so it comes from the active org rather than from a
        // fetch the client-facing page makes with a token this page has not
        // got.
        tenant_name: tenant_name.to_string(),
        logo_url: logo_url.map(str::to_string),
        contact_info: def
            .contact_info
            .clone()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty()),
        rules: def
            .rules
            .iter()
            .filter_map(|r| match r {
                FormRule::RequiredIf {
                    field,
                    when_field,
                    equals,
                } => Some(PublicRule::RequiredIf {
                    field: field.clone(),
                    when_field: when_field.clone(),
                    equals: equals.clone(),
                }),
                FormRule::Other => None,
            })
            .collect(),
        fields: fields
            .into_iter()
            .map(|f| PublicField {
                name: f.name.clone(),
                label: if f.label.trim().is_empty() {
                    f.name.clone()
                } else {
                    f.label.clone()
                },
                help_text: f
                    .help_text
                    .clone()
                    .map(|h| h.trim().to_string())
                    .filter(|h| !h.is_empty()),
                field_type: f.field_type.clone(),
                is_required: f.is_required,
                min_length: f.min_length,
                max_length: f.max_length,
                options: f.options.clone().filter(|o| !o.is_empty()),
                date_not_in_past: f.date_not_in_past,
            })
            .collect(),
    }
}

/// PMS-744: the editor state as the client would receive it.
///
/// Deliberately built from the SIGNALS, not from the saved definition, so the
/// preview answers for what is on screen right now. That is the question being
/// asked ("is this what they will see?"), and it is unanswerable today without
/// issuing a real single-use link to a real client.
///
/// A row too incomplete to render (no reference name) is dropped rather than
/// shown as a blank input: the server would reject the save anyway, and a
/// preview should not invent a field the client will never get.
#[allow(clippy::too_many_arguments)]
fn preview_form(
    name: &str,
    description: &str,
    contact_info: &str,
    tenant_name: &str,
    logo_url: Option<&str>,
    fields: &[FieldRow],
    rules: &[RuleRow],
) -> PublicForm {
    let name = name.trim();
    let description = description.trim();
    PublicForm {
        // An unnamed draft still previews; the placeholder shows where the
        // name will land rather than rendering an empty heading.
        name: if name.is_empty() {
            "Untitled form".to_string()
        } else {
            name.to_string()
        },
        description: (!description.is_empty()).then(|| description.to_string()),
        // PMS-748: previewed from the live editor state too, so an operator
        // adding a contact line sees it land before saving.
        tenant_name: tenant_name.to_string(),
        logo_url: logo_url.map(str::to_string),
        contact_info: {
            let c = contact_info.trim();
            (!c.is_empty()).then(|| c.to_string())
        },
        rules: rules
            .iter()
            .filter(|r| {
                !r.field.trim().is_empty()
                    && !r.when_field.trim().is_empty()
                    && !r.equals.trim().is_empty()
            })
            .map(|r| PublicRule::RequiredIf {
                field: r.field.trim().to_string(),
                when_field: r.when_field.trim().to_string(),
                equals: r.equals.trim().to_string(),
            })
            .collect(),
        fields: fields
            .iter()
            .filter(|f| !f.name.trim().is_empty())
            .map(|f| PublicField {
                name: f.name.trim().to_string(),
                // The client reads the label; falling back to the reference
                // name matches what an unlabelled field would look like.
                label: if f.label.trim().is_empty() {
                    f.name.trim().to_string()
                } else {
                    f.label.trim().to_string()
                },
                help_text: (!f.help_text.trim().is_empty()).then(|| f.help_text.trim().to_string()),
                field_type: f.field_type.as_str().to_string(),
                is_required: f.is_required,
                min_length: None,
                max_length: f.max_length.trim().parse::<i32>().ok(),
                options: f
                    .field_type
                    .needs_options()
                    .then(|| f.parsed_options())
                    .filter(|o| !o.is_empty()),
                date_not_in_past: f.date_not_in_past,
            })
            .collect(),
    }
}

/// The client's view of the form, in a modal over the editor.
///
/// Renders through [`RequestFormBody`], the same component `/request-forms/:token`
/// uses, on the same page background, so what is shown is what is sent. It is
/// live rather than a picture: typing into it exercises the `required_if`
/// rules exactly as a client would hit them.
///
/// The submit button is present but inert. Removing it would misrepresent the
/// page (the client sees one), and wiring it would need a token this form does
/// not have.
#[component]
fn ClientPreviewModal(def: PublicForm, unsaved: bool, onclose: EventHandler<()>) -> Element {
    let answers = use_signal(std::collections::HashMap::<String, String>::new);
    let field_errors = use_signal(std::collections::HashMap::<String, String>::new);
    let empty = def.fields.is_empty();

    rsx! {
        crate::components::Modal {
            open: true,
            title: "Preview: what the client sees".to_string(),
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer: rsx! {
                Button { variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), "Close" }
            },

            if empty {
                p { class: "text-sm text-muted",
                    if unsaved {
                        "Add a field with a reference name to see the client's form."
                    } else {
                        "This form has no fields, so a client would be asked for nothing."
                    }
                }
            } else {
                div { class: "space-y-3",
                    p { class: "text-xs text-muted",
                        if unsaved {
                            "Live preview of unsaved changes. Typing here is not sent anywhere."
                        } else {
                            "Preview of the saved form. Typing here is not sent anywhere."
                        }
                    }
                    // The client's own page background, so spacing and contrast
                    // read the way they will on the real thing.
                    div { class: "bg-app rounded-lg p-6",
                        div { class: "max-w-xl mx-auto bg-surface rounded-lg shadow-lg p-8",
                            RequestFormBody {
                                def: def.clone(),
                                answers,
                                field_errors,
                                form_error: String::new(),
                                disabled: true,
                                submit_label: "Send request".to_string(),
                                onsubmit: move |_| {},
                            }
                        }
                    }
                }
            }
        }
    }
}

/// PMS-760: one section of the editor, in the modal's tab bar.
///
/// Styled after the page-level tabs in `sla.rs` rather than inventing a second
/// tab look. Carries its own item count, and its own problem count from the
/// last save attempt, so a section with something wrong with it says so from
/// whichever tab the operator is standing on.
#[component]
fn EditorTabButton(
    label: String,
    count: Option<usize>,
    problems: usize,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "whitespace-nowrap border-b-2 border-accent px-1 py-2 text-sm font-medium text-accent"
    } else {
        "whitespace-nowrap border-b-2 border-transparent px-1 py-2 text-sm font-medium text-muted hover:border-line hover:text-content"
    };
    rsx! {
        button {
            r#type: "button",
            class: "{class}",
            role: "tab",
            aria_selected: active,
            onclick: move |e| onclick.call(e),
            "{label}"
            if let Some(count) = count {
                span { class: "ml-1.5 rounded-full bg-surface-2 px-1.5 py-0.5 text-xs text-muted", "{count}" }
            }
            if problems > 0 {
                span {
                    class: "ml-1.5 rounded-full bg-red-100 px-1.5 py-0.5 text-xs font-medium text-red-700 dark:bg-red-900 dark:text-red-300",
                    // The number is already beside it; this names what it is
                    // for someone who cannot see the colour.
                    aria_label: "{problems} problems",
                    "{problems}"
                }
            }
        }
    }
}

/// One field, collapsed to a summary until it is being worked on (PMS-760).
///
/// Every control used to be on screen for every field: label, reference name,
/// type, hint, choices, maximum characters, required, and three text links to
/// move or remove the row, each with its own help line. Eight fields was eight
/// of those. Collapsed, a field is one line that says what it is; opening it
/// gives back the full editor, and the controls that exist for edge cases sit
/// behind Advanced inside it.
#[component]
fn FieldRowEditor(
    index: usize,
    row: FieldRow,
    total: usize,
    expanded: bool,
    disabled: bool,
    errors: FieldRowErrors,
    fields: Signal<Vec<FieldRow>>,
    expanded_fields: Signal<HashSet<usize>>,
    /// PMS-760: the row currently being dragged, and the row it is over. Shared
    /// across every row in the list, because a drag has two ends.
    drag_from: Signal<Option<usize>>,
    drag_over: Signal<Option<usize>>,
) -> Element {
    let mut update = move |f: Box<dyn FnOnce(&mut FieldRow)>| {
        if let Some(target) = fields.write().get_mut(index) {
            f(target);
        }
    };

    let type_options: Vec<SelectOption> = FieldType::ALL
        .iter()
        .map(|t| SelectOption::new(t.as_str().to_string(), t.label().to_string()))
        .collect();

    let mut show_advanced = use_signal(|| false);
    // Forced open when the reference name is what is wrong, so a problem can
    // never be reported by a control the operator cannot see. Derived rather
    // than stored, so it follows the error rather than a stale copy of it.
    let advanced_open = show_advanced() || !errors.name.is_empty();

    let has_problem = errors.any();
    let summary = field_summary_label(&row);
    let type_label = row.field_type.label();

    // PMS-760: the row is the drag source, but only once the grip has been
    // pressed. A permanently draggable row swallows text selection inside its
    // own inputs (a drag beats a selection in every browser, and Firefox will
    // not let you select inside a draggable ancestor at all), so `draggable` is
    // armed on mousedown over the grip and disarmed when the drag ends.
    let mut drag_armed = use_signal(|| false);
    let mut drag_from = drag_from;
    let mut drag_over = drag_over;
    let can_drag = !disabled && total > 1;
    let being_dragged = drag_from() == Some(index);
    let is_drop_target = drag_over() == Some(index) && drag_from().is_some_and(|f| f != index);

    // `drop` commits, `dragend` only cleans up. Committing on `dragend` too
    // would look like a useful fallback for a drop the browser refused, but
    // `dragend` is also what fires when the drag is CANCELLED: releasing over
    // nothing, or pressing Escape mid-drag. Reordering someone's fields because
    // they changed their mind is worse than the drag doing nothing.
    let mut commit_drag = move || {
        let Some(from) = drag_from() else {
            return;
        };
        let to = drag_over().unwrap_or(from);
        drag_from.set(None);
        drag_over.set(None);
        drag_armed.set(false);
        if from == to {
            return;
        }
        let moved = expansion_after_move(&expanded_fields.read().clone(), from, to);
        move_row(&mut fields.write(), from, to);
        expanded_fields.set(moved);
    };
    let mut cancel_drag = move || {
        drag_from.set(None);
        drag_over.set(None);
        drag_armed.set(false);
    };

    // A row with a problem is outlined in the state colour. This named a
    // border token input.css never defined, so rows drew no border at all
    // before PMS-760: part of why the list read as one wall.
    let border_class = if has_problem {
        "border-red-300 dark:border-red-600"
    } else if is_drop_target {
        "border-accent"
    } else {
        "border-line"
    };
    let drag_class = if being_dragged {
        " opacity-50"
    } else if is_drop_target {
        // Where it would land, in the accent, without moving anything until
        // the drag is actually let go.
        " ring-2 ring-accent"
    } else {
        ""
    };
    let container_class = format!("rounded-md border {border_class} mb-2{drag_class}");

    rsx! {
        div {
            class: "{container_class}",
            draggable: drag_armed(),
            ondragstart: move |_| {
                drag_from.set(Some(index));
                drag_over.set(Some(index));
            },
            // Every row is a drop target. `prevent_default` on dragover is what
            // makes a drop legal at all: without it the browser refuses the
            // drop and no `drop` event is ever delivered.
            ondragover: move |e| {
                if drag_from().is_some() {
                    e.prevent_default();
                    if drag_over() != Some(index) {
                        drag_over.set(Some(index));
                    }
                }
            },
            ondragenter: move |e| {
                if drag_from().is_some() {
                    e.prevent_default();
                    drag_over.set(Some(index));
                }
            },
            ondrop: move |e| {
                e.prevent_default();
                commit_drag();
            },
            // Fires on the source row after `drop` (which has already cleared
            // the state, so this is a no-op then) and after a cancelled drag,
            // where it is the only thing that puts the row back.
            ondragend: move |_| cancel_drag(),

            // --- summary row -----------------------------------------------
            div { class: "flex items-center gap-2 px-2 py-1.5",
                // The grip. Not a button: it does nothing on click, and a
                // control that is only a cursor affordance would be one more
                // stop for a keyboard user who already has the arrows. Hidden
                // from assistive tech for the same reason, with the drag itself
                // announced by the arrows' labels.
                if can_drag {
                    span {
                        class: "shrink-0 cursor-grab px-0.5 text-subtle hover:text-content active:cursor-grabbing",
                        title: "Drag to reorder",
                        aria_hidden: true,
                        onmousedown: move |_| drag_armed.set(true),
                        onmouseup: move |_| drag_armed.set(false),
                        DragHandleIcon { size: IconSize::Small }
                    }
                }
                button {
                    r#type: "button",
                    class: "flex min-w-0 flex-1 items-center gap-2 rounded px-1 py-1 text-left hover:bg-surface-2",
                    aria_expanded: expanded,
                    onclick: move |_| {
                        let mut open = expanded_fields.write();
                        if !open.remove(&index) {
                            open.insert(index);
                        }
                    },
                    if expanded {
                        ChevronDownIcon { size: IconSize::Small, class: "shrink-0 text-subtle".to_string() }
                    } else {
                        ChevronRightIcon { size: IconSize::Small, class: "shrink-0 text-subtle".to_string() }
                    }
                    span { class: "w-4 shrink-0 text-xs text-subtle", "{index + 1}" }
                    span { class: "truncate text-sm font-medium text-content", "{summary}" }
                    span { class: "shrink-0 text-xs text-muted", "{type_label}" }
                    if row.is_required {
                        Badge { variant: BadgeVariant::Gray, "Required" }
                    }
                    // A collapsed row has no inputs on screen to carry a
                    // message, so the summary says there is one. The messages
                    // themselves are inline, on the controls, once it opens.
                    if has_problem {
                        span { class: "shrink-0 text-xs font-medium text-red-600 dark:text-red-400", "Needs attention" }
                    }
                }
                div { class: "flex shrink-0 items-center gap-1",
                    // PMS-760: two icons rather than three text links. Through
                    // `IconButton`, which requires the accessible name the text
                    // used to be, so shrinking the control does not silently
                    // remove it for a screen reader.
                    IconButton {
                        label: "Move field up".to_string(),
                        class: "p-1 text-subtle hover:text-content".to_string(),
                        disabled: disabled || index == 0,
                        onclick: move |_| {
                            fields.write().swap(index, index - 1);
                            let moved = expansion_after_swap(&expanded_fields.read().clone(), index, index - 1);
                            expanded_fields.set(moved);
                        },
                        ArrowUpIcon { size: IconSize::Small }
                    }
                    IconButton {
                        label: "Move field down".to_string(),
                        class: "p-1 text-subtle hover:text-content".to_string(),
                        disabled: disabled || index + 1 >= total,
                        onclick: move |_| {
                            fields.write().swap(index, index + 1);
                            let moved = expansion_after_swap(&expanded_fields.read().clone(), index, index + 1);
                            expanded_fields.set(moved);
                        },
                        ArrowDownIcon { size: IconSize::Small }
                    }
                    IconButton {
                        // The name opens with the action either way: the label
                        // is the accessible name as well as the tooltip, so the
                        // explanation for a disabled control is appended to it
                        // rather than replacing it.
                        label: if total <= 1 {
                            "Remove field (a form needs at least one field)".to_string()
                        } else {
                            "Remove field".to_string()
                        },
                        class: "p-1 text-subtle hover:text-red-600 dark:hover:text-red-400".to_string(),
                        disabled: disabled || total <= 1,
                        onclick: move |_| {
                            fields.write().remove(index);
                            let left = expansion_after_remove(&expanded_fields.read().clone(), index);
                            expanded_fields.set(left);
                        },
                        TrashIcon { size: IconSize::Small }
                    }
                }
            }

            // --- the field itself ------------------------------------------
            if expanded {
                div { class: "space-y-3 border-t border-line px-3 py-3",
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                        Input {
                            name: "label",
                            label: "Label",
                            value: row.label.clone(),
                            required: true,
                            disabled,
                            error: errors.label.clone(),
                            help: "What the client reads next to the input.".to_string(),
                            oninput: move |e: FormEvent| {
                                let v = e.value();
                                update(Box::new(move |r| {
                                    // PMS-747: the reference name follows the label
                                    // until the operator takes it over, so nobody has
                                    // to invent a payload key to define a field.
                                    if !r.name_touched {
                                        r.name = field_name_from_label(&v);
                                    }
                                    r.label = v;
                                }));
                            },
                        }
                        Select {
                            name: "field_type",
                            label: "Type",
                            options: type_options,
                            value: row.field_type.as_str().to_string(),
                            disabled,
                            onchange: move |e: FormEvent| {
                                let v = e.value();
                                update(Box::new(move |r| {
                                    if let Some(t) = FieldType::from_str(&v) {
                                        r.field_type = t;
                                    }
                                }));
                            },
                        }
                        Input {
                            name: "help_text",
                            label: "Hint",
                            value: row.help_text.clone(),
                            disabled,
                            help: "Optional guidance shown under the input.".to_string(),
                            oninput: move |e: FormEvent| {
                                let v = e.value();
                                update(Box::new(move |r| r.help_text = v));
                            },
                        }

                        // Stays in the open although it is type-specific: a
                        // choice list with no choices is refused by the server,
                        // so it is required rather than advanced.
                        if row.field_type.needs_options() {
                            Input {
                                name: "options",
                                label: "Choices",
                                value: row.options.clone(),
                                required: true,
                                disabled,
                                error: errors.options.clone(),
                                help: "Comma separated. The client must pick one of these.".to_string(),
                                oninput: move |e: FormEvent| {
                                    let v = e.value();
                                    update(Box::new(move |r| r.options = v));
                                },
                            }
                        }
                    }

                    Checkbox {
                        name: "is_required",
                        label: "Required".to_string(),
                        checked: row.is_required,
                        disabled,
                        onchange: move |e: FormEvent| {
                            let on = e.value() == "true" || e.value() == "on";
                            update(Box::new(move |r| r.is_required = on));
                        },
                    }

                    // --- advanced ----------------------------------------------
                    //
                    // The controls that exist for a case rather than for a
                    // field. The reference name is filled in from the label
                    // (PMS-747) and is only touched when answers have to arrive
                    // under a particular key; the rest are optional limits.
                    button {
                        r#type: "button",
                        class: "flex items-center gap-1 text-xs font-medium text-muted hover:text-content",
                        aria_expanded: advanced_open,
                        onclick: move |_| { let next = !show_advanced(); show_advanced.set(next); },
                        if advanced_open {
                            ChevronDownIcon { size: IconSize::Small }
                        } else {
                            ChevronRightIcon { size: IconSize::Small }
                        }
                        "Advanced"
                    }

                    if advanced_open {
                        div { class: "grid grid-cols-1 sm:grid-cols-2 gap-3",
                            Input {
                                name: "name",
                                label: "Reference name",
                                value: row.name.clone(),
                                required: true,
                                disabled,
                                error: errors.name.clone(),
                                help: "Filled in from the label. Change it only if the answers have to arrive under a particular key: on a live form it starts a new column of answers.".to_string(),
                                oninput: move |e: FormEvent| {
                                    let v = e.value();
                                    update(Box::new(move |r| {
                                        r.name_touched = true;
                                        r.name = v;
                                    }));
                                },
                            }
                            if row.field_type.honours_length() {
                                Input {
                                    name: "max_length",
                                    label: "Maximum characters",
                                    r#type: "number".to_string(),
                                    min: Some("1".to_string()),
                                    value: row.max_length.clone(),
                                    disabled,
                                    help: "Optional.".to_string(),
                                    oninput: move |e: FormEvent| {
                                        let v = e.value();
                                        update(Box::new(move |r| r.max_length = v));
                                    },
                                }
                            }
                            if matches!(row.field_type, FieldType::Date) {
                                Checkbox {
                                    name: "date_not_in_past",
                                    label: "Must not be in the past".to_string(),
                                    checked: row.date_not_in_past,
                                    disabled,
                                    onchange: move |e: FormEvent| {
                                        let on = e.value() == "true" || e.value() == "on";
                                        update(Box::new(move |r| r.date_not_in_past = on));
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RuleRowEditor(
    index: usize,
    rule: RuleRow,
    field_names: Vec<SelectOption>,
    disabled: bool,
    rules: Signal<Vec<RuleRow>>,
) -> Element {
    let mut update = move |f: Box<dyn FnOnce(&mut RuleRow)>| {
        if let Some(target) = rules.write().get_mut(index) {
            f(target);
        }
    };

    rsx! {
        div { class: "rounded-md border border-line p-3 mb-3",
            div { class: "flex items-center justify-between mb-2",
                span { class: "text-xs font-medium text-muted", "Rule {index + 1}" }
                IconButton {
                    label: "Remove rule".to_string(),
                    class: "p-1 text-subtle hover:text-red-600 dark:hover:text-red-400".to_string(),
                    disabled,
                    onclick: move |_| { rules.write().remove(index); },
                    TrashIcon { size: IconSize::Small }
                }
            }
            div { class: "grid grid-cols-1 sm:grid-cols-3 gap-3",
                Select {
                    name: "field",
                    label: "Require this field",
                    options: field_names.clone(),
                    value: rule.field.clone(),
                    placeholder: "Choose a field".to_string(),
                    disabled,
                    onchange: move |e: FormEvent| {
                        let v = e.value();
                        update(Box::new(move |r| r.field = v));
                    },
                }
                Select {
                    name: "when_field",
                    label: "When this field",
                    options: field_names.clone(),
                    value: rule.when_field.clone(),
                    placeholder: "Choose a field".to_string(),
                    disabled,
                    onchange: move |e: FormEvent| {
                        let v = e.value();
                        update(Box::new(move |r| r.when_field = v));
                    },
                }
                Input {
                    name: "equals",
                    label: "Equals",
                    value: rule.equals.clone(),
                    required: true,
                    disabled,
                    help: "Match a choice exactly.".to_string(),
                    oninput: move |e: FormEvent| {
                        let v = e.value();
                        update(Box::new(move |r| r.equals = v));
                    },
                }
            }
        }
    }
}

fn optional(v: &str) -> Option<String> {
    let t = v.trim();
    (!t.is_empty()).then(|| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_matches_the_servers_slug_shape() {
        assert_eq!(slugify("New starter"), "new-starter");
        assert_eq!(slugify("  Leaver / offboarding  "), "leaver-offboarding");
        assert_eq!(slugify("Move desk (2026)"), "move-desk-2026");
        assert_eq!(
            slugify("!!!"),
            "",
            "a name with nothing sluggable yields an empty slug the operator must fill in"
        );
    }

    /// PMS-747: the operator names the field; the payload key follows.
    #[test]
    fn a_reference_name_is_derived_from_the_label() {
        assert_eq!(field_name_from_label("Phone number"), "phone_number");
        assert_eq!(field_name_from_label("  Last name  "), "last_name");
        assert_eq!(field_name_from_label("E-mail / work"), "e_mail_work");
        assert_eq!(
            field_name_from_label("2nd contact"),
            "f_2nd_contact",
            "a leading digit is prefixed, not dropped: `nd_contact` no longer reads like its label"
        );
        assert_eq!(
            field_name_from_label("!!!"),
            "",
            "a label with nothing derivable leaves the operator to name it, and the row says so"
        );
    }

    /// The point of deriving is that the result never has to be corrected, so
    /// the derivation must satisfy the check the server applies.
    #[test]
    fn a_derived_reference_name_is_one_the_server_accepts() {
        for label in [
            "Phone number",
            "2nd contact",
            "E-mail / work",
            "Serial #",
            "Do you need a laptop?",
        ] {
            let derived = field_name_from_label(label);
            assert_eq!(
                field_name_problem(&derived),
                None,
                "`{label}` derived `{derived}`, which the server would reject"
            );
        }
    }

    /// PMS-747: a hand-edited name is checked here rather than by a 400 that
    /// lands in a banner the operator has already scrolled past.
    #[test]
    fn a_hand_typed_reference_name_is_checked_before_the_request() {
        assert_eq!(field_name_problem("phone_number"), None);
        assert!(field_name_problem("").is_some(), "empty");
        assert!(field_name_problem("Last Name").is_some(), "capitals, space");
        assert!(field_name_problem("1st_line").is_some(), "leading digit");
        assert!(field_name_problem("trailing_").is_some(), "trailing _");
        assert!(field_name_problem("double__bar").is_some(), "doubled _");
    }

    /// PMS-754: a draft round-trips through storage without changing what the
    /// operator typed, including the per-field state that is not part of the
    /// server payload.
    /// This module's own source, minus the test module that names the strings
    /// it asserts on.
    fn production_src() -> &'static str {
        const FORMS_SRC: &str = include_str!("forms.rs");
        FORMS_SRC
            .split_once("#[cfg(test)]")
            .map(|(before, _)| before)
            .expect("this file has a test module")
    }

    /// PMS-756 recurrence guard.
    ///
    /// The close prompt shipped with two ways out, neither of which saved, so
    /// the operator had to cancel out of it and find the footer button. The
    /// prompt fires at the moment someone has decided to leave; the most likely
    /// version of that intent has to be on it.
    #[test]
    fn the_close_prompt_offers_all_three_ways_out() {
        let src = production_src();
        for action in ["Save and close", "Close without saving", "Keep editing"] {
            assert!(
                src.contains(action),
                "the close prompt must offer `{action}`"
            );
        }
    }

    #[test]
    fn a_draft_round_trips_through_its_stored_shape() {
        let state = EditorState {
            id: Some("11111111-1111-1111-1111-111111111111".into()),
            name: "New starter".into(),
            slug: "new-starter".into(),
            description: "  ".into(),
            contact_info: "the service desk".into(),
            kb_article_id: "22222222-2222-2222-2222-222222222222".into(),
            is_active: false,
            fields: vec![FieldRow {
                name: "kind".into(),
                label: "Kind".into(),
                field_type: FieldType::Select,
                options: "new, reuse".into(),
                is_required: true,
                // Hand-edited, so the label must not silently rewrite it when
                // the draft comes back.
                name_touched: true,
                ..FieldRow::new()
            }],
            rules: vec![RuleRow {
                field: "kind".into(),
                when_field: "kind".into(),
                equals: "new".into(),
            }],
            has_unknown_rule: false,
        };

        let json = serde_json::to_string(&DraftForm::from_state(&state)).expect("serialise");
        let back: DraftForm = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.into_state(&state), state);
    }

    fn draft_named(name: &str) -> DraftForm {
        DraftForm {
            name: name.to_string(),
            ..DraftForm::default()
        }
    }

    fn server_draft(name: &str, updated_at: &str) -> ServerDraft {
        ServerDraft {
            id: "33333333-3333-3333-3333-333333333333".into(),
            form_definition_id: None,
            name: Some(name.to_string()),
            payload: draft_named(name),
            updated_at: updated_at.to_string(),
        }
    }

    /// PMS-759: the restore decision, which is the one place this feature can
    /// silently lose work. Neither tier is authoritative: the local copy is
    /// written on every change and so is usually ahead on the machine that
    /// typed it, and the server copy is the only thing that exists anywhere
    /// else.
    #[test]
    fn the_newer_draft_wins_whichever_tier_it_is_in() {
        // 2026-08-11T00:00:00Z, and one hour later.
        let earlier = "2026-08-11T00:00:00Z";
        let later_ms = 1_786_500_000_000.0;

        // Local typed after the server's copy: keep what is in this browser.
        let picked = newest_draft(
            Some((draft_named("local"), later_ms)),
            Some(&server_draft("remote", earlier)),
        );
        assert_eq!(picked.expect("a draft").name, "local");

        // The other machine typed last: take theirs.
        let picked = newest_draft(
            Some((draft_named("local"), 0.0)),
            Some(&server_draft("remote", earlier)),
        );
        assert_eq!(picked.expect("a draft").name, "remote");
    }

    /// The cross-device case: a browser that has never seen this form restores
    /// the server's copy. Without this the feature is exactly the localStorage
    /// one it was asked to replace.
    #[test]
    fn a_browser_with_no_local_draft_restores_the_servers() {
        let picked = newest_draft(None, Some(&server_draft("remote", "2026-08-11T00:00:00Z")));
        assert_eq!(picked.expect("a draft").name, "remote");
        assert!(newest_draft(None, None).is_none());
    }

    /// A timestamp that will not parse must not be treated as "now": that
    /// would replace local work with a copy of unknown age. It reads as
    /// arbitrarily old instead, so the browser keeps what it has.
    #[test]
    fn an_unreadable_server_timestamp_does_not_beat_local_work() {
        let picked = newest_draft(
            Some((draft_named("local"), 1.0)),
            Some(&server_draft("remote", "not a timestamp")),
        );
        assert_eq!(picked.expect("a draft").name, "local");
    }

    /// PMS-759 kept the stored shape and wrapped it, so a draft written by an
    /// earlier build still parses. It reads as arbitrarily old, which is the
    /// right way round: a bare draft predates server drafts entirely.
    #[test]
    fn a_legacy_local_draft_still_parses_and_yields_to_the_server() {
        let legacy = serde_json::to_string(&draft_named("legacy")).expect("serialise");
        let parsed: DraftForm =
            serde_json::from_str(&legacy).expect("a bare draft is still readable");
        assert_eq!(parsed.name, "legacy");

        let stored = StoredDraft {
            saved_at: 42.0,
            form: draft_named("stamped"),
        };
        let json = serde_json::to_string(&stored).expect("serialise");
        let back: StoredDraft = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back.saved_at, 42.0);
        assert_eq!(back.form.name, "stamped");
    }

    /// A draft is normally started before it is named, so the list needs a
    /// label for the untitled case rather than rendering an empty row.
    #[test]
    fn an_unnamed_draft_still_has_something_to_show_in_the_list() {
        let mut d = server_draft("", "2026-08-11T00:00:00Z");
        d.name = None;
        assert_eq!(d.label(), "Untitled form");
        d.name = Some("   ".into());
        assert_eq!(d.label(), "Untitled form");
        d.name = Some("Leaver".into());
        assert_eq!(d.label(), "Leaver");
    }

    /// Identity comes from the definition the server sent, never from storage:
    /// a draft must not be able to retarget which definition is being edited,
    /// nor clear the flag that blocks saving a rule this build cannot render.
    #[test]
    fn a_draft_cannot_retarget_the_definition_it_restores_into() {
        let draft = DraftForm {
            name: "Typed while offline".into(),
            ..DraftForm::default()
        };
        let base = EditorState {
            id: Some("33333333-3333-3333-3333-333333333333".into()),
            has_unknown_rule: true,
            ..EditorState::new()
        };

        let restored = draft.into_state(&base);
        assert_eq!(restored.id, base.id);
        assert!(restored.has_unknown_rule);
        assert_eq!(restored.name, "Typed while offline");
    }

    /// PMS-754: two definitions edited in one browser must not share a draft,
    /// and a new form needs a key of its own.
    #[test]
    fn each_definition_gets_its_own_draft_key() {
        assert_ne!(draft_key(Some("a")), draft_key(Some("b")));
        assert_ne!(draft_key(None), draft_key(Some("a")));
        assert!(draft_key(None).ends_with(".new"));
    }

    /// PMS-748: the client's page names the MSP and, optionally, how to reach
    /// them. The preview is only worth having if it carries the same, so an
    /// operator signing one off sees what their client will read.
    #[test]
    fn the_preview_carries_the_attribution_the_client_will_see() {
        let fields = vec![FieldRow {
            name: "first_name".into(),
            label: "First name".into(),
            ..FieldRow::new()
        }];

        let previewed = preview_form(
            "Starter",
            "",
            "  the service desk on 555-0100  ",
            "Acme IT",
            Some("/api/v1/public/tenants/1/logo"),
            &fields,
            &[],
        );
        assert_eq!(previewed.tenant_name, "Acme IT");
        assert_eq!(
            previewed.logo_url.as_deref(),
            Some("/api/v1/public/tenants/1/logo"),
            "MAPPS-429: the client sees the logo, so the preview has to as well"
        );
        assert_eq!(
            previewed.contact_info.as_deref(),
            Some("the service desk on 555-0100"),
            "the client is served a trimmed value, so the preview must be too"
        );

        let blank = preview_form("Starter", "", "   ", "Acme IT", None, &fields, &[]);
        assert_eq!(
            blank.contact_info, None,
            "a contact field holding only spaces must not preview as a contact line"
        );
    }

    #[test]
    fn options_are_split_and_trimmed_and_blanks_dropped() {
        let row = FieldRow {
            options: " new , reuse existing ,, none ".to_string(),
            ..FieldRow::new()
        };
        assert_eq!(
            row.parsed_options(),
            vec!["new", "reuse existing", "none"],
            "an empty entry from a trailing comma must not become an empty choice"
        );
    }

    #[test]
    fn a_saved_definition_previews_in_sort_order_without_unknown_rules() {
        use crate::modules::forms::FormField;

        let field = |name: &str, sort_order: i32| FormField {
            id: uuid::Uuid::nil(),
            name: name.into(),
            label: String::new(),
            help_text: None,
            field_type: "text".into(),
            is_required: false,
            min_length: None,
            max_length: None,
            options: None,
            date_not_in_past: false,
            sort_order,
        };
        let def = FormDefinition {
            id: uuid::Uuid::nil(),
            name: "Starter".into(),
            slug: "starter".into(),
            description: Some("   ".into()),
            contact_info: None,
            kb_article_id: None,
            kb_article_title: None,
            rules: vec![
                FormRule::Other,
                FormRule::RequiredIf {
                    field: "b".into(),
                    when_field: "a".into(),
                    equals: "yes".into(),
                },
            ],
            is_active: true,
            // Arrival order deliberately not sort order: the client is served
            // them sorted, so the preview must sort too.
            fields: vec![field("b", 2), field("a", 1)],
        };

        let preview = preview_from_definition(&def, "Acme IT", None);

        assert_eq!(
            preview
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(
            preview.fields[0].label, "a",
            "an unlabelled field shows its reference name, as the client would see it"
        );
        assert_eq!(
            preview.rules.len(),
            1,
            "a rule this build cannot render is dropped"
        );
        assert_eq!(
            preview.description, None,
            "a whitespace-only description would render as an empty line"
        );
    }

    #[test]
    fn the_preview_drops_rows_the_client_would_never_receive() {
        // A half-typed field has no reference name, so the server would reject
        // the save; showing it in the preview would promise the client a field
        // that cannot exist. Same for a rule missing any of its three parts.
        let fields = vec![
            FieldRow {
                name: "first_name".into(),
                label: "First name".into(),
                ..FieldRow::new()
            },
            FieldRow {
                label: "Half typed".into(),
                ..FieldRow::new()
            },
        ];
        let rules = vec![RuleRow {
            field: "first_name".into(),
            when_field: "".into(),
            equals: "yes".into(),
        }];

        let preview = preview_form("Starter", "", "", "Acme IT", None, &fields, &rules);
        assert_eq!(preview.fields.len(), 1);
        assert_eq!(preview.fields[0].name, "first_name");
        assert!(
            preview.rules.is_empty(),
            "an incomplete rule cannot fire, so previewing it would mislead"
        );
    }

    #[test]
    fn the_preview_falls_back_where_the_client_would_see_a_gap() {
        let fields = vec![FieldRow {
            name: "note".into(),
            label: "   ".into(),
            ..FieldRow::new()
        }];
        let preview = preview_form("  ", "  ", "  ", "Acme IT", None, &fields, &[]);

        assert_eq!(preview.name, "Untitled form");
        assert_eq!(preview.description, None);
        assert_eq!(
            preview.fields[0].label, "note",
            "an unlabelled field shows its reference name, which is what the client would get"
        );
    }

    #[test]
    fn only_a_choice_list_carries_options() {
        let fields = vec![
            FieldRow {
                name: "kind".into(),
                field_type: FieldType::Select,
                options: "new, reuse".into(),
                ..FieldRow::new()
            },
            FieldRow {
                name: "detail".into(),
                field_type: FieldType::Text,
                // Left over from switching the type back to text; the client
                // never sees these, so neither does the preview.
                options: "stale, values".into(),
                max_length: "120".into(),
                ..FieldRow::new()
            },
        ];
        let preview = preview_form("Kinds", "", "", "Acme IT", None, &fields, &[]);

        assert_eq!(
            preview.fields[0].options.as_deref(),
            Some(["new".to_string(), "reuse".to_string()].as_slice())
        );
        assert_eq!(preview.fields[1].options, None);
        assert_eq!(preview.fields[1].max_length, Some(120));
    }

    #[test]
    fn a_retired_definition_is_not_offered_for_sending() {
        let def = FormDefinition {
            id: uuid::Uuid::nil(),
            name: "Old starter".into(),
            slug: "old-starter".into(),
            description: None,
            contact_info: None,
            kb_article_id: None,
            kb_article_title: None,
            rules: Vec::new(),
            is_active: false,
            fields: Vec::new(),
        };
        assert!(
            !is_sendable(&def),
            "the server refuses submissions on a retired form, so a link issued for one would die on arrival"
        );
        assert!(is_sendable(&FormDefinition {
            is_active: true,
            ..def
        }));
    }

    #[test]
    fn an_unknown_rule_kind_is_flagged_rather_than_silently_dropped() {
        let def = FormDefinition {
            id: uuid::Uuid::nil(),
            name: "Departure".into(),
            slug: "departure".into(),
            description: None,
            contact_info: None,
            kb_article_id: None,
            kb_article_title: None,
            rules: vec![FormRule::Other],
            is_active: true,
            fields: Vec::new(),
        };
        let state = EditorState::from_existing(&def);
        assert!(
            state.has_unknown_rule,
            "a rule this build cannot represent must block the save, not vanish on it"
        );
        assert!(state.rules.is_empty());
    }

    // ========================================================================
    // PMS-760: density
    // ========================================================================

    /// A failed save must land on the section holding the problem. The footer
    /// count is the only thing certain to be on screen (PMS-747), and a count
    /// that points at a tab the operator is not on is worse than no count.
    #[test]
    fn a_failed_save_lands_on_the_section_that_failed() {
        assert_eq!(
            ProblemCounts {
                details: 1,
                fields: 2,
                rules: 0
            }
            .first_section(),
            Some(EditorSection::Details),
            "the first section with a problem wins, in the order they are shown"
        );
        assert_eq!(
            ProblemCounts {
                details: 0,
                fields: 3,
                rules: 1
            }
            .first_section(),
            Some(EditorSection::Fields)
        );
        assert_eq!(
            ProblemCounts {
                details: 0,
                fields: 0,
                rules: 1
            }
            .first_section(),
            Some(EditorSection::Rules)
        );
        assert_eq!(
            ProblemCounts::default().first_section(),
            None,
            "a clean save must not move the operator off the section they were working on"
        );
        assert_eq!(
            ProblemCounts {
                details: 1,
                fields: 2,
                rules: 3
            }
            .total(),
            6,
            "the footer total must be the sum of what the tabs report, or one of them is lying"
        );
    }

    /// Expansion is keyed by position, so reordering has to carry it along.
    /// Otherwise moving an open field up leaves the field it displaced showing
    /// as open, with the operator's half-typed row collapsed under it.
    #[test]
    fn an_open_field_stays_open_when_it_moves() {
        let expanded = HashSet::from([2usize]);
        assert_eq!(
            expansion_after_swap(&expanded, 2, 1),
            HashSet::from([1usize]),
            "the open row moved up, so the open position moves with it"
        );
        assert_eq!(
            expansion_after_swap(&HashSet::from([1usize]), 2, 1),
            HashSet::from([2usize]),
            "and the row it displaced keeps its own state"
        );
        assert_eq!(
            expansion_after_swap(&HashSet::from([0usize, 3]), 1, 2),
            HashSet::from([0usize, 3]),
            "rows either side of a swap are untouched"
        );
    }

    /// Removing a row shifts everything after it down one. Without this,
    /// removing field 1 would leave field 3 open and field 2 collapsed.
    #[test]
    fn removing_a_field_shifts_the_open_rows_below_it() {
        assert_eq!(
            expansion_after_remove(&HashSet::from([0usize, 2, 3]), 1),
            HashSet::from([0usize, 1, 2])
        );
        assert_eq!(
            expansion_after_remove(&HashSet::from([1usize]), 1),
            HashSet::new(),
            "the removed row takes its own entry with it"
        );
        assert_eq!(
            expansion_after_remove(&HashSet::from([0usize]), 2),
            HashSet::from([0usize]),
            "rows above the removed one do not move"
        );
    }

    /// A collapsed row has to say what it is. A field is normally given its
    /// type before its label, so the unnamed case is ordinary rather than an
    /// edge, and an empty summary row does not look clickable.
    #[test]
    fn a_collapsed_field_always_has_something_to_show() {
        let mut row = FieldRow::new();
        assert_eq!(field_summary_label(&row), "Untitled field");
        row.name = "phone_number".to_string();
        assert_eq!(
            field_summary_label(&row),
            "phone_number",
            "the reference name stands in until the label is typed"
        );
        row.label = "  Phone number  ".to_string();
        assert_eq!(
            field_summary_label(&row),
            "Phone number",
            "the label is what the client reads, so it is what the row is called"
        );
    }

    /// A drag moves a row and closes the gap behind it. The arrow buttons swap
    /// two rows, which is right for a single step and wrong for a drag:
    /// dropping field 5 onto field 1 must put it at 1 and push the rest down,
    /// not exchange the two and scramble everything between them.
    #[test]
    fn a_dragged_row_lands_where_it_was_dropped() {
        let mut rows = vec!["a", "b", "c", "d", "e"];
        move_row(&mut rows, 4, 1);
        assert_eq!(rows, vec!["a", "e", "b", "c", "d"]);

        let mut rows = vec!["a", "b", "c", "d"];
        move_row(&mut rows, 0, 2);
        assert_eq!(rows, vec!["b", "c", "a", "d"], "and downwards too");

        let mut rows = vec!["a", "b"];
        move_row(&mut rows, 1, 1);
        assert_eq!(rows, vec!["a", "b"], "a drag that ends where it started");
        move_row(&mut rows, 0, 9);
        assert_eq!(rows, vec!["a", "b"], "an index off the end changes nothing");
    }

    /// Expansion is keyed by position, so it has to shift exactly the way the
    /// rows do. Otherwise dragging an open field leaves some other row showing
    /// as open in its place.
    #[test]
    fn dragging_a_row_carries_its_open_state() {
        assert_eq!(
            expansion_after_move(&HashSet::from([4usize]), 4, 1),
            HashSet::from([1usize]),
            "the dragged row's own state lands on its new index"
        );
        assert_eq!(
            expansion_after_move(&HashSet::from([1usize, 2]), 4, 1),
            HashSet::from([2usize, 3]),
            "rows the drag pushed down move with them"
        );
        assert_eq!(
            expansion_after_move(&HashSet::from([1usize, 2]), 0, 2),
            HashSet::from([0usize, 1]),
            "and rows pulled up when the drag went the other way"
        );
        assert_eq!(
            expansion_after_move(&HashSet::from([0usize, 3]), 1, 2),
            HashSet::from([0usize, 3]),
            "rows outside the moved span are untouched"
        );
        assert_eq!(
            expansion_after_move(&HashSet::from([2usize]), 2, 2),
            HashSet::from([2usize]),
            "a drag that ends where it started"
        );
    }

    /// MAPPS-434: adding a field opens it and closes the one before it, so a
    /// form built one field at a time does not end as the wall PMS-760 removed.
    #[test]
    fn adding_a_field_opens_only_the_new_one() {
        assert_eq!(
            expansion_after_add(3),
            HashSet::from([3usize]),
            "the new row is open and nothing else is"
        );
        assert_eq!(
            expansion_after_add(0),
            HashSet::from([0usize]),
            "including the first field of an empty form"
        );
    }

    /// PMS-760: a row with a problem is opened by the failed save, and the
    /// summary marks it. Both of those hang off `any()`.
    #[test]
    fn a_row_with_a_problem_is_distinguishable_from_a_clean_one() {
        assert!(!FieldRowErrors::default().any());
        assert!(FieldRowErrors {
            name: "A reference name is required.".to_string(),
            ..Default::default()
        }
        .any());
    }
}

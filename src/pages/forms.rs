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

use dioxus::prelude::*;

use crate::components::{
    use_page_title, Badge, BadgeVariant, Button, ButtonVariant, Checkbox, DataTable, ErrorBanner,
    Input, PageHeader, Select, SelectOption, Table, TableBody, TableCell, TableEmpty, TableHead,
    TableHeader, TableLoading, TableRow, Textarea,
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

    let mut editing = use_signal(|| None::<EditorState>);
    // MAPPS-424: (definition id, name) of the form being sent, if any.
    let mut sending = use_signal(|| None::<(String, String)>);
    // PMS-744: the client's view of a saved definition, opened from its row.
    let mut previewing = use_signal(|| None::<PublicForm>);

    let snap = forms.read_unchecked();
    let is_loading = snap.is_none();
    let fetch_failed = matches!(*snap, Some(None));
    let rows: Vec<FormDefinition> = match &*snap {
        Some(Some(list)) => list.clone(),
        _ => Vec::new(),
    };
    let total = rows.len();

    let reachable = crate::hooks::use_server_reachable();
    let can_mutate = crate::hooks::use_can_mutate();
    // PMS-748: the client's page names the MSP, so the preview of it has to
    // name the same MSP. The operator IS the tenant here, so it comes from the
    // active org rather than from the token-scoped public endpoint.
    let tenant_name = active_org_name();
    if fetch_failed && !reachable {
        return rsx! {
            crate::components::ContentUnavailable { title: "Request Forms".to_string() }
        };
    }

    rsx! {
        PageHeader { title: "Request Forms".to_string(),
            // MAPPS-424: the subtitle says what a definition is FOR, because
            // defining one is never the goal. Without it the page ends at Save
            // with no sign that a form reaches a client from somewhere else.
            subtitle: "Forms clients fill in to raise a request. Each one becomes a ticket carrying its knowledge base article. Define a form once here, then use Send to email a client a link to it.".to_string(),
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
                                let for_preview = preview_from_definition(&def, &tenant_name);
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

        if let Some(state) = editing.read().clone() {
            FormEditorModal {
                state,
                onclose: move |_| { editing.set(None); },
                onsaved: move |_| {
                    editing.set(None);
                    forms.restart();
                },
            }
        }

        if let Some((id, name)) = sending.read().clone() {
            crate::pages::request_links::SendFormToClientModal {
                form_definition_id: id,
                form_name: name,
                onclose: move |_| { sending.set(None); },
                onsent: move |_| { sending.set(None); },
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

/// PMS-748: the active org's display name, or an empty string before the
/// membership list has loaded. An empty name suppresses the attribution block
/// rather than previewing a form "sent to you by " nobody.
fn active_org_name() -> String {
    let auth = crate::hooks::use_auth();
    let name = auth
        .read()
        .active_org_name()
        .unwrap_or_default()
        .to_string();
    name
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

// ============================================================================
// EDITOR MODAL
// ============================================================================

#[component]
fn FormEditorModal(
    state: EditorState,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
) -> Element {
    let is_edit = state.id.is_some();
    let save_id = state.id.clone();

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
    let mut problem_count = use_signal(|| 0usize);
    // PMS-744: preview of the client's view, built from the live editor state.
    let mut previewing = use_signal(|| false);
    // PMS-748: see `FormsBuilderPage`; the preview must name the MSP the
    // client's own page will name.
    let tenant_name = active_org_name();

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

    let handle_save = move |_| {
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
        field_errors.set(row_errors);

        // A rule pointing at a field that no longer exists can never fire, and
        // the server rejects it, so it is caught here for the same reason.
        let rule_rows = rules.read().clone();
        for (i, r) in rule_rows.iter().enumerate() {
            let n = i + 1;
            if r.field.trim().is_empty() || r.when_field.trim().is_empty() {
                row_problems.push(format!("Rule {n} needs both fields chosen."));
            }
            if r.equals.trim().is_empty() {
                row_problems.push(format!("Rule {n} needs a value to match."));
            }
        }

        if has_unknown_rule {
            row_problems.push(
                "This form has a rule this version cannot edit. Saving would remove it, so it is blocked. Update the app first.".to_string(),
            );
        }

        if !row_problems.is_empty() {
            error.set(row_problems.join(" "));
            failed = true;
        }
        // Counts the top-level slots too, so "1 problem" cannot mean an empty
        // Name field the operator was never pointed at.
        problem_count.set(
            row_problems.len()
                + usize::from(!name_error.read().is_empty())
                + usize::from(!slug_error.read().is_empty()),
        );
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
    };

    let can_mutate = crate::hooks::use_can_mutate();
    let footer = rsx! {
        // PMS-747: the footer is pinned while the body scrolls, so this is the
        // only place a failed Create is certain to be seen. Without it, pressing
        // Create at the bottom of a three-field form set an error banner several
        // hundred pixels above the viewport and looked like a dead button.
        if problem_count() > 0 {
            span {
                class: "mr-auto self-center text-sm text-danger",
                role: "alert",
                if problem_count() == 1 {
                    "1 problem to fix above."
                } else {
                    "{problem_count()} problems to fix above."
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
        Button { variant: ButtonVariant::Secondary, onclick: move |_| onclose.call(()), "Cancel" }
        Button {
            variant: ButtonVariant::Primary,
            loading: saving(),
            disabled: !can_mutate,
            title: (!can_mutate).then(|| "Can't save while the server is unreachable".to_string()),
            onclick: handle_save,
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

    rsx! {
        crate::components::Modal {
            open: true,
            title: if is_edit { "Edit request form".to_string() } else { "New request form".to_string() },
            size: crate::components::ModalSize::Large,
            onclose: move |_| onclose.call(()),
            footer,

            div { class: "space-y-6",

                if !error().is_empty() {
                    ErrorBanner { "{error()}" }
                }

                // --- definition ------------------------------------------------
                div { class: "space-y-4",
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
                    // PMS-750: a plain heading, not a header row. The control
                    // that adds a field lives under the last row; see below.
                    h3 { class: "text-sm font-semibold text-content mb-2", "Fields" }
                    p { class: "text-xs text-muted mb-3",
                        "Order here is the order the client sees. The reference name is the key the answer is stored under, so changing it on a live form starts a new column of answers."
                    }

                    for (index, row) in fields.read().clone().into_iter().enumerate() {
                        FieldRowEditor {
                            key: "{index}",
                            index,
                            row,
                            total: fields.read().len(),
                            disabled: saving(),
                            errors: field_errors.read().get(index).cloned().unwrap_or_default(),
                            fields,
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
                        onclick: move |_| fields.write().push(FieldRow::new()),
                        "+ Add field"
                    }
                }

                // --- rules -----------------------------------------------------
                div {
                    div { class: "flex items-center justify-between mb-2",
                        h3 { class: "text-sm font-semibold text-content", "Conditional rules" }
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

        if previewing() {
            ClientPreviewModal {
                def: preview_form(
                    &name(),
                    &description(),
                    &contact_info(),
                    &tenant_name,
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
fn preview_from_definition(def: &FormDefinition, tenant_name: &str) -> PublicForm {
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

#[component]
fn FieldRowEditor(
    index: usize,
    row: FieldRow,
    total: usize,
    disabled: bool,
    errors: FieldRowErrors,
    fields: Signal<Vec<FieldRow>>,
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

    rsx! {
        div { class: "rounded-md border border-default p-3 mb-3 space-y-3",
            div { class: "flex items-center justify-between",
                span { class: "text-xs font-medium text-muted", "Field {index + 1}" }
                div { class: "flex gap-2",
                    Button {
                        variant: ButtonVariant::Link,
                        disabled: disabled || index == 0,
                        onclick: move |_| { fields.write().swap(index, index - 1); },
                        "Move up"
                    }
                    Button {
                        variant: ButtonVariant::Link,
                        disabled: disabled || index + 1 >= total,
                        onclick: move |_| { fields.write().swap(index, index + 1); },
                        "Move down"
                    }
                    Button {
                        variant: ButtonVariant::Link,
                        disabled: disabled || total <= 1,
                        title: (total <= 1).then(|| "A form needs at least one field".to_string()),
                        onclick: move |_| { fields.write().remove(index); },
                        "Remove"
                    }
                }
            }

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
                Input {
                    name: "name",
                    label: "Reference name",
                    value: row.name.clone(),
                    required: true,
                    disabled,
                    error: errors.name.clone(),
                    help: "Filled in from the label. Change it only if the answers have to arrive under a particular key.".to_string(),
                    oninput: move |e: FormEvent| {
                        let v = e.value();
                        update(Box::new(move |r| {
                            r.name_touched = true;
                            r.name = v;
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
            }

            div { class: "flex flex-wrap gap-4",
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
        div { class: "rounded-md border border-default p-3 mb-3",
            div { class: "flex items-center justify-between mb-2",
                span { class: "text-xs font-medium text-muted", "Rule {index + 1}" }
                Button {
                    variant: ButtonVariant::Link,
                    disabled,
                    onclick: move |_| { rules.write().remove(index); },
                    "Remove"
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
            &fields,
            &[],
        );
        assert_eq!(previewed.tenant_name, "Acme IT");
        assert_eq!(
            previewed.contact_info.as_deref(),
            Some("the service desk on 555-0100"),
            "the client is served a trimmed value, so the preview must be too"
        );

        let blank = preview_form("Starter", "", "   ", "Acme IT", &fields, &[]);
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

        let preview = preview_from_definition(&def, "Acme IT");

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

        let preview = preview_form("Starter", "", "", "Acme IT", &fields, &rules);
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
        let preview = preview_form("  ", "  ", "  ", "Acme IT", &fields, &[]);

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
        let preview = preview_form("Kinds", "", "", "Acme IT", &fields, &[]);

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
}

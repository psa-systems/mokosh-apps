//! PMS-730: `/request-forms/:token`, the page a client's request-form link
//! opens.
//!
//! mokosh-server emails `{app_url}/request-forms/{token}`
//! (`src/modules/forms/request_links.rs`), serves the form at
//! `GET /api/v1/public/request-forms/{token}` and takes the submission at
//! `POST` on the same path. The token IS the credential, so this page sends no
//! bearer: the visitor is a client with no session of any kind.
//!
//! Server status contract, mirrored in the copy below so a client can tell the
//! cases apart:
//!
//! - 200 -> render the form
//! - 410 -> the link was already submitted (their request is already with us)
//! - 400 -> expired, unknown or malformed; the server deliberately does not
//!   distinguish these, so neither does this page
//! - 422 -> per-field validation errors, routed to their inputs
//! - 429 -> rate limited
//!
//! The field set, its validation and the cross-field rules are all defined
//! server-side (PMS-731); this page renders whatever it is handed rather than
//! hard-coding any MACD knowledge.

use std::collections::HashMap;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::{
    Button, ButtonVariant, Checkbox, DateField, Input, Select, SelectOption, Textarea,
};

/// One field of the form, mirroring mokosh-server's `PublicFormField`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
struct PublicField {
    name: String,
    label: String,
    #[serde(default)]
    help_text: Option<String>,
    field_type: String,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    min_length: Option<i32>,
    #[serde(default)]
    max_length: Option<i32>,
    #[serde(default)]
    options: Option<Vec<String>>,
    #[serde(default)]
    date_not_in_past: bool,
}

/// A cross-field rule, mirroring mokosh-server's `FormRule`. One kind exists
/// (`required_if`); an unknown kind deserialises into `Other` and is ignored
/// here, so a server that grows a rule this build predates degrades to
/// server-side enforcement rather than failing to render the form.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PublicRule {
    RequiredIf {
        field: String,
        when_field: String,
        equals: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct PublicForm {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    rules: Vec<PublicRule>,
    fields: Vec<PublicField>,
}

#[derive(Serialize)]
struct SubmitBody {
    payload: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct Receipt {
    ticket_number: String,
}

/// Terminal states the page can land in, each with its own copy. Kept as an
/// enum rather than a pile of booleans so it is impossible to render the form
/// and a terminal message at the same time.
#[derive(Debug, Clone, PartialEq)]
enum Terminal {
    /// Submitted successfully; carries the ticket number to quote.
    Submitted(String),
    /// 410: this link has already been used.
    AlreadySubmitted,
    /// 400: expired, unknown or malformed.
    Unusable,
}

#[component]
pub fn RequestFormPage(token: String) -> Element {
    let token = use_signal(|| token);

    // The form definition, loaded once from the token.
    let form = use_signal(|| None::<PublicForm>);
    let loading = use_signal(|| true);
    let terminal = use_signal(|| None::<Terminal>);

    // Answers keyed by field name. Booleans are held as "true"/"false" strings
    // so one map covers every field type; `build_payload` converts them back to
    // real JSON types on submit, because the server type-checks each field.
    let answers = use_signal(HashMap::<String, String>::new);
    let field_errors = use_signal(HashMap::<String, String>::new);
    let form_error = use_signal(String::new);
    let submitting = use_signal(|| false);

    // Load the form behind the link.
    use_effect({
        let mut form = form;
        let mut loading = loading;
        let mut terminal = terminal;
        move || {
            let tok = token.read().clone();
            spawn(async move {
                #[cfg(feature = "web")]
                {
                    use crate::hooks::fetch::api::ApiError;
                    match crate::hooks::fetch::api::get_typed::<PublicForm>(&format!(
                        "/public/request-forms/{tok}"
                    ))
                    .await
                    {
                        Ok(f) => form.set(Some(f)),
                        Err(ApiError::Status { code: 410, .. }) => {
                            terminal.set(Some(Terminal::AlreadySubmitted))
                        }
                        Err(ApiError::Status { code: 400, .. }) => {
                            terminal.set(Some(Terminal::Unusable))
                        }
                        // Anything else (network, 5xx, 429) is not terminal:
                        // the link may well work on a retry, so the visitor is
                        // told to try again rather than that their link is dead.
                        Err(_) => terminal.set(None),
                    }
                }
                #[cfg(not(feature = "web"))]
                {
                    let _ = tok;
                }
                loading.set(false);
            });
        }
    });

    let mut handle_submit = {
        let mut field_errors = field_errors;
        let mut form_error = form_error;
        let mut submitting = submitting;
        let mut terminal = terminal;
        move |_| {
            if submitting() {
                return;
            }
            let Some(def) = form.read().clone() else {
                return;
            };
            let current = answers.read().clone();

            // Repo convention (docs/form-conventions.md): evaluate EVERY
            // required field and set each failed slot, then bail once. Never
            // short-circuit, or one missing field masks another. Whitespace is
            // not an answer, matching the server, which trims before deciding
            // whether a field was answered at all.
            let mut errs: HashMap<String, String> = HashMap::new();
            for f in &def.fields {
                let answered = current
                    .get(&f.name)
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false);
                // A boolean is answered by existing: `false` is a real answer,
                // so an unticked required checkbox is still "answered".
                let answered = answered || f.field_type == "boolean";
                if (f.is_required || required_by_rule(&def.rules, &f.name, &current)) && !answered {
                    errs.insert(f.name.clone(), format!("{} is required", f.label));
                }
            }
            field_errors.set(errs.clone());
            if !errs.is_empty() {
                form_error.set(String::new());
                return;
            }

            submitting.set(true);
            form_error.set(String::new());
            let tok = token.read().clone();
            let payload = build_payload(&def, &current);

            spawn(async move {
                #[cfg(feature = "web")]
                {
                    use crate::hooks::fetch::api::ApiError;
                    let body = SubmitBody { payload };
                    match crate::hooks::fetch::api::post_typed::<Receipt, _>(
                        &format!("/public/request-forms/{tok}"),
                        &body,
                    )
                    .await
                    {
                        Ok(r) => terminal.set(Some(Terminal::Submitted(r.ticket_number))),
                        // Per-field rules the client could not check (pattern,
                        // option set, date-not-in-past) come back here and are
                        // routed to their inputs, so the client fixes the field
                        // rather than reading a banner.
                        Err(e @ ApiError::Status { code: 422, .. }) => {
                            let mut routed = HashMap::new();
                            let mut unrouted = Vec::new();
                            for fe in e.field_errors() {
                                if def.fields.iter().any(|f| f.name == fe.field) {
                                    routed.insert(fe.field.clone(), fe.message.clone());
                                } else {
                                    // A message about a field this form does not
                                    // render (or about the payload as a whole)
                                    // would be invisible if only routed.
                                    unrouted.push(fe.message.clone());
                                }
                            }
                            field_errors.set(routed);
                            form_error.set(unrouted.join(" "));
                        }
                        Err(ApiError::Status { code: 410, .. }) => {
                            terminal.set(Some(Terminal::AlreadySubmitted))
                        }
                        Err(ApiError::Status { code: 400, .. }) => {
                            terminal.set(Some(Terminal::Unusable))
                        }
                        Err(ApiError::Status { code: 429, .. }) => form_error
                            .set("Too many attempts. Wait a moment and try again.".to_string()),
                        Err(e) => form_error.set(e.user_message()),
                    }
                }
                #[cfg(not(feature = "web"))]
                {
                    let _ = (tok, payload);
                }
                submitting.set(false);
            });
        }
    };

    rsx! {
        div { class: "min-h-screen bg-app flex items-center justify-center px-4 py-10",
            div { class: "max-w-xl w-full",
                div { class: "bg-surface rounded-lg shadow-lg p-8",
                    match terminal() {
                        Some(Terminal::Submitted(number)) => rsx! {
                            div { class: "text-center", role: "status", aria_live: "polite",
                                h1 { class: "text-2xl font-semibold text-content", "Request received" }
                                p { class: "mt-2 text-sm text-content",
                                    "Thanks. Your request is with us as ticket "
                                    span { class: "font-mono font-medium", "{number}" }
                                    ". Quote that number if you need to follow it up."
                                }
                            }
                        },
                        Some(Terminal::AlreadySubmitted) => rsx! {
                            div { class: "text-center", role: "status", aria_live: "polite",
                                h1 { class: "text-2xl font-semibold text-content", "Already submitted" }
                                p { class: "mt-2 text-sm text-content",
                                    "This link has already been used, so your request is with us. Ask your account team if you need to send another."
                                }
                            }
                        },
                        Some(Terminal::Unusable) => rsx! {
                            div { class: "text-center", role: "status", aria_live: "polite",
                                h1 { class: "text-2xl font-semibold text-content", "Link expired" }
                                p { class: "mt-2 text-sm text-content",
                                    "This link is expired or invalid. Ask your account team for a new one."
                                }
                            }
                        },
                        None if loading() => rsx! {
                            p { class: "text-center text-sm text-content", "Loading your form..." }
                        },
                        None => match form() {
                            None => rsx! {
                                div { class: "text-center",
                                    h1 { class: "text-2xl font-semibold text-content", "Something went wrong" }
                                    p { class: "mt-2 text-sm text-content",
                                        "We could not load your form. Check your connection and reload the page."
                                    }
                                }
                            },
                            Some(def) => rsx! {
                                div { class: "mb-6",
                                    h1 { class: "text-2xl font-semibold text-content", "{def.name}" }
                                    if let Some(d) = def.description.clone() {
                                        p { class: "mt-2 text-sm text-content", "{d}" }
                                    }
                                }

                                if !form_error().is_empty() {
                                    div {
                                        class: "mb-4 rounded-md border border-danger px-3 py-2 text-sm text-danger",
                                        role: "alert",
                                        "{form_error()}"
                                    }
                                }

                                form {
                                    class: "space-y-4",
                                    onsubmit: move |evt: Event<FormData>| {
                                        evt.prevent_default();
                                        handle_submit(());
                                    },

                                    for field in def.fields.clone() {
                                        FieldInput {
                                            key: "{field.name}",
                                            field: field.clone(),
                                            rules: def.rules.clone(),
                                            answers,
                                            field_errors,
                                            disabled: submitting(),
                                        }
                                    }

                                    Button {
                                        variant: ButtonVariant::Primary,
                                        r#type: "submit".to_string(),
                                        disabled: submitting(),
                                        class: "w-full".to_string(),
                                        if submitting() { "Sending..." } else { "Send request" }
                                    }
                                }
                            },
                        },
                    }
                }
            }
        }
    }
}

/// One rendered field. Split out so the match on `field_type` lives in one
/// place and the parent stays about flow rather than widgets.
#[component]
fn FieldInput(
    field: PublicField,
    rules: Vec<PublicRule>,
    answers: Signal<HashMap<String, String>>,
    field_errors: Signal<HashMap<String, String>>,
    disabled: bool,
) -> Element {
    let name = field.name.clone();
    let value = answers.read().get(&name).cloned().unwrap_or_default();
    let error = field_errors.read().get(&name).cloned().unwrap_or_default();
    let help = field.help_text.clone().unwrap_or_default();

    // A field the server will require because of a cross-field rule is marked
    // required as soon as its condition holds, so the asterisk appears the
    // moment the answer it depends on is given rather than only on submit.
    let required = field.is_required || required_by_rule(&rules, &name, &answers.read());

    let mut set = {
        let name = name.clone();
        move |v: String| {
            answers.write().insert(name.clone(), v);
            field_errors.write().remove(&name);
        }
    };

    match field.field_type.as_str() {
        "textarea" => rsx! {
            Textarea {
                name: field.name.clone(),
                label: field.label.clone(),
                value,
                required,
                disabled,
                error,
                help,
                maxlength: field.max_length.map(|m| m as i64),
                oninput: move |e: FormEvent| set(e.value()),
            }
        },
        "date" => rsx! {
            DateField {
                name: field.name.clone(),
                label: field.label.clone(),
                value,
                required,
                disabled,
                error,
                help,
                oninput: move |e: FormEvent| set(e.value()),
            }
        },
        "select" => rsx! {
            Select {
                name: field.name.clone(),
                label: field.label.clone(),
                options: field
                    .options
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|o| SelectOption::new(o.clone(), o))
                    .collect::<Vec<_>>(),
                value,
                placeholder: "Choose one".to_string(),
                required,
                disabled,
                error,
                help,
                onchange: move |e: FormEvent| set(e.value()),
            }
        },
        "boolean" => rsx! {
            Checkbox {
                name: field.name.clone(),
                label: field.label.clone(),
                checked: value == "true",
                disabled,
                error,
                help,
                onchange: move |e: FormEvent| {
                    // A checkbox reports "true"/"false" through the same string
                    // map every other field uses; `build_payload` turns it back
                    // into a JSON bool, which is what the server type-checks.
                    let checked = e.value() == "true" || e.value() == "on";
                    set(checked.to_string());
                },
            }
        },
        // "text", "email", and anything a newer server grows that this build
        // does not know: render a text input rather than dropping the field.
        // The server is the authority on validation either way, so an unknown
        // type degrades to "collect a string and let the server judge it".
        _ => rsx! {
            Input {
                name: field.name.clone(),
                label: field.label.clone(),
                r#type: if field.field_type == "email" { "email".to_string() } else { "text".to_string() },
                value,
                required,
                disabled,
                error,
                help,
                maxlength: field.max_length.map(|m| m as i64),
                oninput: move |e: FormEvent| set(e.value()),
            }
        },
    }
}

/// Whether a cross-field rule makes `field` required given the current
/// answers. Mirrors `required_by_rule` in mokosh-server's
/// `src/modules/forms/validation.rs`; the server remains the authority, this
/// only spares the client a round trip.
fn required_by_rule(rules: &[PublicRule], field: &str, answers: &HashMap<String, String>) -> bool {
    rules.iter().any(|r| match r {
        PublicRule::RequiredIf {
            field: target,
            when_field,
            equals,
        } => {
            target == field
                && answers
                    .get(when_field)
                    .map(|v| v.trim() == equals)
                    .unwrap_or(false)
        }
        PublicRule::Other => false,
    })
}

/// Convert the string-keyed answers into the JSON the server type-checks:
/// booleans as real booleans, everything else as trimmed strings, and blanks
/// omitted entirely rather than sent as `""`.
fn build_payload(def: &PublicForm, answers: &HashMap<String, String>) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for f in &def.fields {
        let Some(raw) = answers.get(&f.name) else {
            continue;
        };
        if f.field_type == "boolean" {
            out.insert(f.name.clone(), serde_json::Value::Bool(raw == "true"));
            continue;
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            // The server treats blank as absent; sending "" would just make it
            // say so on the round trip.
            continue;
        }
        out.insert(
            f.name.clone(),
            serde_json::Value::String(trimmed.to_string()),
        );
    }
    serde_json::Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> PublicForm {
        PublicForm {
            name: "Departure".into(),
            description: None,
            rules: vec![PublicRule::RequiredIf {
                field: "forward_to".into(),
                when_field: "mailbox_handling".into(),
                equals: "forward".into(),
            }],
            fields: vec![
                PublicField {
                    name: "employee_name".into(),
                    label: "Employee name".into(),
                    help_text: None,
                    field_type: "text".into(),
                    is_required: true,
                    min_length: None,
                    max_length: None,
                    options: None,
                    date_not_in_past: false,
                },
                PublicField {
                    name: "equipment_moves".into(),
                    label: "Equipment moves".into(),
                    help_text: None,
                    field_type: "boolean".into(),
                    is_required: false,
                    min_length: None,
                    max_length: None,
                    options: None,
                    date_not_in_past: false,
                },
            ],
        }
    }

    #[test]
    fn required_if_tracks_the_condition_field() {
        let mut answers = HashMap::new();
        assert!(!required_by_rule(&form().rules, "forward_to", &answers));

        answers.insert("mailbox_handling".to_string(), "convert to shared".into());
        assert!(!required_by_rule(&form().rules, "forward_to", &answers));

        answers.insert("mailbox_handling".to_string(), "forward".into());
        assert!(required_by_rule(&form().rules, "forward_to", &answers));
    }

    #[test]
    fn an_unknown_rule_kind_is_ignored_rather_than_breaking_the_form() {
        let rules: Vec<PublicRule> =
            serde_json::from_str(r#"[{"kind":"invented_later","field":"x"}]"#)
                .expect("an unknown rule kind still deserialises");
        assert_eq!(rules, vec![PublicRule::Other]);
        assert!(!required_by_rule(&rules, "x", &HashMap::new()));
    }

    #[test]
    fn payload_sends_real_booleans_and_omits_blanks() {
        let mut answers = HashMap::new();
        answers.insert("employee_name".to_string(), "  Dana  ".to_string());
        answers.insert("equipment_moves".to_string(), "false".to_string());
        let payload = build_payload(&form(), &answers);

        assert_eq!(payload["employee_name"], serde_json::json!("Dana"));
        assert_eq!(
            payload["equipment_moves"],
            serde_json::json!(false),
            "a checkbox must send a JSON bool, not the string \"false\""
        );

        answers.insert("employee_name".to_string(), "   ".to_string());
        let blank = build_payload(&form(), &answers);
        assert!(
            blank.get("employee_name").is_none(),
            "a whitespace-only answer is omitted, matching the server's own trim"
        );
    }
}

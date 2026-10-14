//! Click-to-edit field primitive for the phase-2 asset drawer
//! (MAPPS-xxxx asset inline edit; docs/mokosh-assets/03-inline-edit.md).
//!
//! The drawer renders one `InlineFieldEditor` per editable asset field.
//! The display mode shows the value as plain text with a subtle hover
//! cue; clicking swaps it into an input / select / textarea. Blur or
//! Enter commits via PATCH; Esc cancels. On save the parent is handed
//! the new value through `on_patched` so it can refresh derived state
//! without a second fetch.
//!
//! This is deliberately generic (not asset-specific) so phase 2 of
//! other list surfaces (tickets, contacts) can reuse the same shape.

use dioxus::prelude::*;

use crate::hooks::fetch::api::ApiError;

#[derive(Clone, PartialEq, Debug)]
pub enum InlineFieldKind {
    /// Single-line text. Validation: trimmed, non-empty when `required`.
    Text {
        placeholder: String,
        maxlength: Option<i64>,
        required: bool,
    },
    /// Multi-line text. No length cap at the client; the server rules
    /// the row size.
    TextArea { placeholder: String, rows: u8 },
    /// Native date input (`type="date"`). Server receives `YYYY-MM-DD`
    /// or `null` for clear-when-optional.
    Date,
    /// Dropdown. `options` is `(value, label)`; an empty value doubles
    /// as the "clear" affordance when `optional`.
    Select { options: Vec<(String, String)> },
}

#[derive(Props, Clone, PartialEq)]
pub struct InlineFieldEditorProps {
    pub label: String,
    /// Current value as the server stores it. `None` renders as "-".
    pub value: Option<String>,
    pub kind: InlineFieldKind,
    /// `PATCH <path>` is the HTTP target; the body is
    /// `{ "<patch_field>": <new value or null> }`. The parent is
    /// responsible for interpolating any id into `path`.
    pub path: String,
    pub patch_field: String,
    /// When true, saving `""` (or clicking "clear" in a Select) sends
    /// `null` so the server writes NULL; when false, that is a
    /// validation error.
    #[props(default = false)]
    pub optional: bool,
    /// Called with the new raw-string value after the server has
    /// confirmed the save. The parent typically writes it back to its
    /// `use_resource` snapshot so a reopen of the drawer shows the
    /// fresh value without a second fetch.
    pub on_patched: EventHandler<Option<String>>,
    /// Optional display mapper for `Select { options }`: the stored
    /// value may be a uuid while the display label is "Acme Corp". The
    /// SPA passes a (value -> label) resolver here; `None` falls back
    /// to the stored string.
    #[props(default)]
    pub display: Option<String>,
}

#[component]
pub fn InlineFieldEditor(props: InlineFieldEditorProps) -> Element {
    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| props.value.clone().unwrap_or_default());
    let mut saving = use_signal(|| false);
    let mut err = use_signal(String::new);

    let display_text = props
        .display
        .clone()
        .or_else(|| props.value.clone())
        .unwrap_or_else(|| "-".to_string());

    let path = props.path.clone();
    let patch_field = props.patch_field.clone();
    let optional = props.optional;
    let on_patched = props.on_patched;

    // `use_callback` gives a `Copy` handle so the closure can be cloned
    // into every editor's `onblur`/`onkeydown` without a per-site move.
    let commit = use_callback(move |raw: String| {
        let path = path.clone();
        let patch_field = patch_field.clone();
        spawn(async move {
            saving.set(true);
            err.set(String::new());
            let trimmed = raw.trim().to_string();
            if trimmed.is_empty() && !optional {
                err.set("Required".to_string());
                saving.set(false);
                return;
            }
            let body_value = if trimmed.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(trimmed.clone())
            };
            let body = serde_json::json!({ &patch_field: body_value });
            match crate::hooks::fetch::api::patch_authed_typed::<serde_json::Value, _>(&path, &body)
                .await
            {
                Ok(_) => {
                    saving.set(false);
                    editing.set(false);
                    on_patched.call(if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    });
                }
                Err(ApiError::Status {
                    message, fields, ..
                }) => {
                    let field_msg = fields
                        .into_iter()
                        .find(|f| f.field == patch_field)
                        .map(|f| f.message)
                        .unwrap_or(message);
                    err.set(if field_msg.is_empty() {
                        "Save failed".to_string()
                    } else {
                        field_msg
                    });
                    saving.set(false);
                }
                Err(e) => {
                    err.set(e.user_message());
                    saving.set(false);
                }
            }
        });
    });

    let label = props.label.clone();
    let kind = props.kind.clone();
    let current = props.value.clone().unwrap_or_default();

    rsx! {
        div { class: "mb-3",
            label {
                class: "block text-xs font-medium text-muted mb-1",
                "{label}"
            }
            if editing() {
                div { class: "flex items-start gap-2",
                    div { class: "flex-1",
                        match kind {
                            InlineFieldKind::Text { placeholder, maxlength, required: _ } => rsx! {
                                input {
                                    r#type: "text",
                                    class: "block w-full rounded-md border border-line bg-surface text-content \
                                            px-2 py-1 text-sm shadow-sm focus:border-accent focus:ring-accent",
                                    value: "{draft.read()}",
                                    placeholder,
                                    maxlength,
                                    oninput: move |e: FormEvent| draft.set(e.value()),
                                    onkeydown: move |e: KeyboardEvent| {
                                        let key = e.key();
                                        if matches!(key, Key::Enter) {
                                            commit.call(draft.read().clone());
                                        } else if matches!(key, Key::Escape) {
                                            editing.set(false);
                                            draft.set(current.clone());
                                            err.set(String::new());
                                        }
                                    },
                                    onblur: move |_| commit.call(draft.read().clone()),
                                    autofocus: true,
                                }
                            },
                            InlineFieldKind::TextArea { placeholder, rows } => rsx! {
                                textarea {
                                    class: "block w-full rounded-md border border-line bg-surface text-content \
                                            px-2 py-1 text-sm shadow-sm focus:border-accent focus:ring-accent",
                                    value: "{draft.read()}",
                                    placeholder,
                                    rows: rows as i64,
                                    oninput: move |e: FormEvent| draft.set(e.value()),
                                    onkeydown: move |e: KeyboardEvent| {
                                        if matches!(e.key(), Key::Escape) {
                                            editing.set(false);
                                            draft.set(current.clone());
                                            err.set(String::new());
                                        }
                                    },
                                    onblur: move |_| commit.call(draft.read().clone()),
                                    autofocus: true,
                                }
                            },
                            InlineFieldKind::Date => rsx! {
                                input {
                                    r#type: "date",
                                    class: "block w-full rounded-md border border-line bg-surface text-content \
                                            px-2 py-1 text-sm shadow-sm focus:border-accent focus:ring-accent",
                                    value: "{draft.read()}",
                                    oninput: move |e: FormEvent| draft.set(e.value()),
                                    onkeydown: move |e: KeyboardEvent| {
                                        if matches!(e.key(), Key::Enter) {
                                            commit.call(draft.read().clone());
                                        } else if matches!(e.key(), Key::Escape) {
                                            editing.set(false);
                                            draft.set(current.clone());
                                            err.set(String::new());
                                        }
                                    },
                                    onblur: move |_| commit.call(draft.read().clone()),
                                    autofocus: true,
                                }
                            },
                            InlineFieldKind::Select { options } => rsx! {
                                select {
                                    class: "block w-full rounded-md border border-line bg-surface text-content \
                                            px-2 py-1 text-sm shadow-sm focus:border-accent focus:ring-accent",
                                    value: "{draft.read()}",
                                    onchange: move |e: FormEvent| {
                                        draft.set(e.value());
                                        commit.call(e.value());
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if matches!(e.key(), Key::Escape) {
                                            editing.set(false);
                                            draft.set(current.clone());
                                            err.set(String::new());
                                        }
                                    },
                                    autofocus: true,
                                    if optional {
                                        option { value: "", "-" }
                                    }
                                    for (v, l) in options.iter() {
                                        option { key: "{v}", value: "{v}", "{l}" }
                                    }
                                }
                            },
                        }
                        if !err.read().is_empty() {
                            p { class: "mt-1 text-xs text-red-600 dark:text-red-400",
                                "{err}"
                            }
                        }
                    }
                    if saving() {
                        span { class: "text-xs text-subtle whitespace-nowrap pt-1.5",
                            "Saving..."
                        }
                    }
                }
            } else {
                button {
                    r#type: "button",
                    class: "block w-full text-left rounded px-2 py-1 text-sm text-content \
                            hover:bg-surface-2 border border-transparent hover:border-line",
                    onclick: move |_| {
                        draft.set(current.clone());
                        err.set(String::new());
                        editing.set(true);
                    },
                    "{display_text}"
                }
            }
        }
    }
}

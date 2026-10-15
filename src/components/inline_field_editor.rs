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
    /// Optional per-host registry of which editor is currently in edit
    /// mode. The host (drawer) creates one `Signal<Option<String>>` and
    /// passes it to every editor; each editor compares its own
    /// `patch_field` against the signal to decide whether to render
    /// edit mode. Clicking a different editor flips the signal and
    /// closes the previously open one, which is the behaviour browser
    /// blur alone does not guarantee when focus moves between inputs
    /// in the same component tree. When omitted (any host that only
    /// renders one editor at a time), the component falls back to its
    /// own local `editing` signal and the behaviour is unchanged.
    #[props(default)]
    pub open_field: Option<Signal<Option<String>>>,
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
    let open_patch_field = patch_field.clone();
    let close_patch_field = patch_field.clone();
    let is_editing_patch_field = patch_field.clone();
    let optional = props.optional;
    let on_patched = props.on_patched;
    // Per-host "which editor is open" registry. When a parent passes
    // one (the drawer does), this editor renders edit mode only while
    // the registry names its own `patch_field`. Clicking a different
    // editor flips the registry and this editor's render drops to
    // read mode on the next pass, which closes it even when the
    // browser's blur event fails to fire between focus moves inside
    // the same component tree. Without a registry, falls back to the
    // local `editing` signal and the behaviour is unchanged.
    let open_field = props.open_field;
    let is_editing_now = match open_field {
        Some(sig) => sig.read().as_deref() == Some(is_editing_patch_field.as_str()),
        None => editing(),
    };

    // `use_callback` gives a `Copy` handle so the closure can be cloned
    // into every editor's `onblur`/`onkeydown` without a per-site move.
    let commit = use_callback(move |raw: String| {
        if saving() {
            return;
        }
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
                    if let Some(mut sig) = open_field {
                        if sig.read().as_deref() == Some(patch_field.as_str()) {
                            sig.set(None);
                        }
                    }
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

    // Close helper shared by Esc on every kind + the "clear draft on
    // registry hand-off" path. Flips both the local signal (so a host
    // without a registry still sees the state change) and the
    // registry (so a host that does, drops the "which editor is open"
    // entry when this editor is the one closing). `use_callback` gives
    // a `Copy` handle so each event closure can take it without a
    // per-site clone.
    let close_editor = use_callback(move |()| {
        editing.set(false);
        if let Some(mut sig) = open_field {
            if sig.read().as_deref() == Some(close_patch_field.as_str()) {
                sig.set(None);
            }
        }
    });

    rsx! {
        div { class: "mb-3",
            label {
                class: "block text-xs font-medium text-muted mb-1",
                "{label}"
            }
            if is_editing_now {
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
                                    disabled: saving(),
                                    oninput: move |e: FormEvent| draft.set(e.value()),
                                    onkeydown: move |e: KeyboardEvent| {
                                        let key = e.key();
                                        if matches!(key, Key::Enter) {
                                            commit.call(draft.read().clone());
                                        } else if matches!(key, Key::Escape) {
                                            close_editor.call(());
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
                                    disabled: saving(),
                                    oninput: move |e: FormEvent| draft.set(e.value()),
                                    onkeydown: move |e: KeyboardEvent| {
                                        if matches!(e.key(), Key::Escape) {
                                            close_editor.call(());
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
                                    disabled: saving(),
                                    oninput: move |e: FormEvent| draft.set(e.value()),
                                    onkeydown: move |e: KeyboardEvent| {
                                        if matches!(e.key(), Key::Enter) {
                                            commit.call(draft.read().clone());
                                        } else if matches!(e.key(), Key::Escape) {
                                            close_editor.call(());
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
                                    disabled: saving(),
                                    onchange: move |e: FormEvent| {
                                        draft.set(e.value());
                                        commit.call(e.value());
                                    },
                                    onkeydown: move |e: KeyboardEvent| {
                                        if matches!(e.key(), Key::Escape) {
                                            close_editor.call(());
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
                            "Saving…"
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
                        if let Some(mut sig) = open_field {
                            sig.set(Some(open_patch_field.clone()));
                        }
                    },
                    "{display_text}"
                }
            }
        }
    }
}

/// MAPPS-1054: `commit` is a `use_callback` shared by every keystroke/blur
/// handler, so a fast double-Enter (or Enter then blur) can fire it twice
/// before the first `spawn`ed PATCH resolves. The regression is a re-entrant
/// `commit` invocation issuing a second `patch_authed_typed` call while one is
/// already in flight.
///
/// Scanned as source rather than driven through a rendered `VirtualDom`,
/// mirroring `toggle_capability_and_poll_save_toggle_the_same_busy_signal_connect_does`
/// in `settings_integrations.rs` (MAPPS-1019): `saving` is a component-local
/// signal with no handle exposed outside a full Dioxus render, and the real
/// commit path is a network PATCH this test suite has no server to answer, so
/// the guard that prevents the second call is checked where it lives instead.
#[cfg(test)]
mod mapps1054_commit_guard_tests {
    /// `needle` must end with the opening `{` of the block to capture, so the
    /// brace this starts counting from is unambiguous even when `needle`
    /// itself contains earlier, unrelated braces (as the `InlineFieldKind`
    /// match-arm patterns do).
    fn block_body<'a>(src: &'a str, needle: &str) -> &'a str {
        assert!(
            needle.ends_with('{'),
            "{needle} must end with the block's opening brace"
        );
        let at = src
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} exists"));
        let after = &src[at..];
        let brace_at = needle.len() - 1;
        let mut depth = 0i32;
        let mut end = None;
        for (i, ch) in after[brace_at..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(brace_at + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        &after[..end.expect("the block closes")]
    }

    #[test]
    fn commit_returns_early_when_a_save_is_already_in_flight() {
        let src = include_str!("inline_field_editor.rs");
        let commit = block_body(src, "let commit = use_callback(move |raw: String| {");
        let guard_at = commit
            .find("if saving() {")
            .expect("commit must check saving() before doing any work");
        let spawn_at = commit
            .find("spawn(async move {")
            .expect("commit spawns the PATCH");
        assert!(
            guard_at < spawn_at,
            "the in-flight guard must run before spawn, or a re-entrant call still races the PATCH"
        );
        let guard_block = block_body(&commit[guard_at..], "if saving() {");
        assert!(
            guard_block.contains("return;"),
            "commit must return immediately rather than queue behind the in-flight save"
        );
    }

    #[test]
    fn every_editable_control_disables_itself_while_saving() {
        let src = include_str!("inline_field_editor.rs");
        for (kind, needle) in [
            (
                "Text",
                "InlineFieldKind::Text { placeholder, maxlength, required: _ } => rsx! {",
            ),
            (
                "TextArea",
                "InlineFieldKind::TextArea { placeholder, rows } => rsx! {",
            ),
            ("Date", "InlineFieldKind::Date => rsx! {"),
            ("Select", "InlineFieldKind::Select { options } => rsx! {"),
        ] {
            let block = block_body(src, needle);
            assert!(
                block.contains("disabled: saving()"),
                "{kind}'s control must carry `disabled: saving()` so a user cannot trigger \
                 a second keystroke-driven commit while one is outstanding"
            );
        }
    }
}

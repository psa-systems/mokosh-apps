//! A Markdown source field with its toolbar, shortcuts and mention completion
//! (MAPPS-592).
//!
//! The KB editor grew all of this over MAPPS-579, MAPPS-580 and MAPPS-587, and
//! it stayed welded into that one page. Every other place the app takes
//! Markdown, notably the ticket description, was a bare `<textarea>`: the same
//! syntax, rendered by the same component afterwards, with none of the help
//! that made the KB editor usable. This is that write pane, minus the parts
//! that belong to an article.
//!
//! What is deliberately NOT here:
//!
//! - **A preview.** The KB editor's Write/Preview tabs and side-by-side split
//!   are layout owned by that page, and a ticket description is edited in a
//!   modal that has nowhere to put a second pane. Left out on the reporter's
//!   own instruction.
//! - **Uploading.** `on_file` is passed through to the toolbar and is `None`
//!   for every host but the KB editor, because the upload route belongs to an
//!   article and a ticket description has nothing to attach a file to. The
//!   toolbar's Image dialog stays URL-only when it is absent.
//!
//! The field is still a plain `<textarea>` holding Markdown source. That is the
//! MAPPS-579 decision and it has not changed: the source is the single source
//! of truth and every control edits it.

use dioxus::prelude::*;

use super::form::Textarea;
use super::markdown_toolbar::{run_action, shortcut_action, MarkdownToolbar};
use super::mention_autocomplete::MentionAutocomplete;
use crate::utils::mentions::Mention;
use crate::utils::validation::Rule;

#[derive(Props, Clone, PartialEq)]
pub struct MarkdownEditorProps {
    /// `name` of the field, which `Textarea` also renders as its `id`. The
    /// toolbar and the mention list both address the real DOM node by it, so it
    /// has to be unique on the page.
    pub name: String,
    /// Field label.
    pub label: String,
    #[props(default)]
    pub placeholder: String,
    #[props(default = 8)]
    pub rows: u32,
    #[props(default = false)]
    pub required: bool,
    #[props(default = false)]
    pub disabled: bool,
    #[props(default)]
    pub rules: Vec<Rule>,
    #[props(default)]
    pub maxlength: Option<i64>,
    /// Error to show under the field. Empty lets the field validate itself.
    #[props(default)]
    pub error: String,
    /// The source. This component does not own it; the host does.
    pub value: String,
    /// Fires with the new source, whatever changed it: typing, a toolbar
    /// button, a keyboard shortcut, or an accepted mention.
    pub oninput: EventHandler<String>,
    /// MAPPS-580: the directory `@handle` completes against. Empty disables
    /// completion, which is the right default for a host that has no directory
    /// loaded: the renderer resolves a mention either way.
    #[props(default)]
    pub people: Vec<Mention>,
    /// MAPPS-587: where a picked file goes. See the module header.
    #[props(default)]
    pub on_file: Option<EventHandler<(String, String, Vec<u8>)>>,
    /// MAPPS-594: keep the label out of the flow while still using it.
    ///
    /// For a host that already names the field, such as a card whose title is
    /// "Description" wrapping the description editor. The label still names the
    /// field in a validation message and still gives the textarea its
    /// accessible name; it is only the second visible copy that goes.
    #[props(default = false)]
    pub label_hidden: bool,
    /// Extra classes on the wrapper.
    #[props(default)]
    pub class: String,
}

#[component]
pub fn MarkdownEditor(props: MarkdownEditorProps) -> Element {
    // MAPPS-579: Ctrl+K lands on the textarea but opens a dialog that belongs
    // to the toolbar, so the request travels as a flag the toolbar consumes.
    let mut link_shortcut = use_signal(|| false);

    let target = props.name.clone();
    let on_change = props.oninput;

    rsx! {
        div { class: "space-y-1 {props.class}",
            // The label belongs above the toolbar AND the field, because the
            // two are one control. `Textarea` would otherwise draw it between
            // them, where it reads as a caption on the toolbar. Still a real
            // `<label for>`, so the field keeps its accessible name.
            if !props.label.is_empty() && !props.label_hidden {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-content",
                    "{props.label}"
                    if props.required {
                        span {
                            class: "text-red-500 dark:text-red-400 ml-1",
                            aria_label: "required",
                            role: "img",
                            "*"
                        }
                    }
                }
            }
            MarkdownToolbar {
                target_id: props.name.clone(),
                value: props.value.clone(),
                disabled: props.disabled,
                open_link: link_shortcut,
                on_file: props.on_file,
                onchange: move |next: String| on_change.call(next),
            }
            Textarea {
                name: props.name.clone(),
                label: props.label.clone(),
                // Always hidden on the field itself: this component draws the
                // label above the toolbar, or the host does.
                label_hidden: true,
                placeholder: props.placeholder.clone(),
                rows: props.rows,
                required: props.required,
                disabled: props.disabled,
                maxlength: props.maxlength,
                rules: props.rules.clone(),
                error: props.error.clone(),
                // The toolbar draws the top border and corners, so the field
                // joins onto it instead of sitting in its own box below one.
                class: "rounded-t-none resize-y".to_string(),
                value: props.value.clone(),
                oninput: move |e: FormEvent| on_change.call(e.value()),
                onkeydown: {
                    let target = target.clone();
                    let value = props.value.clone();
                    move |e: KeyboardEvent| {
                        let mods = e.modifiers();
                        let chord = mods.ctrl() || mods.meta();
                        let key = match e.key() {
                            Key::Character(c) => c,
                            _ => String::new(),
                        };
                        // Link opens a dialog rather than applying a transform,
                        // so it is handled here while the marks go through the
                        // shared mapping.
                        if chord && key.eq_ignore_ascii_case("k") {
                            e.prevent_default();
                            link_shortcut.set(true);
                            return;
                        }
                        if let Some(action) = shortcut_action(chord, &key) {
                            e.prevent_default();
                            let handler = EventHandler::new(move |next: String| {
                                on_change.call(next);
                            });
                            run_action(&target, &value, &action, &handler);
                        }
                    }
                },
            }
            // MAPPS-580: the mention list, under the field it completes for.
            // Renders nothing unless an `@` is being typed that the RENDERER
            // would also read as a mention.
            MentionAutocomplete {
                target_id: props.name.clone(),
                value: props.value.clone(),
                people: props.people.clone(),
                onaccept: {
                    let target = props.name.clone();
                    move |(text, caret): (String, u32)| {
                        on_change.call(text);
                        let target = target.clone();
                        spawn(async move {
                            crate::platform::timer::sleep_ms(0).await;
                            crate::platform::dom::set_textarea_selection(&target, caret, caret);
                        });
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("markdown_editor.rs");

    fn code_only() -> String {
        let end = SRC
            .find("mod tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Everything the KB write pane had, minus the two things that belong to an
    /// article. A host that mounts this gets the whole set or the component is
    /// not worth sharing.
    #[test]
    fn the_editor_carries_the_toolbar_the_shortcuts_and_the_mentions() {
        let code = code_only();
        assert!(code.contains("MarkdownToolbar {"), "the toolbar");
        assert!(
            code.contains("shortcut_action(chord, &key)"),
            "the shortcuts"
        );
        assert!(code.contains("MentionAutocomplete {"), "mention completion");
    }

    /// MAPPS-579: Ctrl+K is the one shortcut that does not transform text, so
    /// it cannot go through `shortcut_action`. It has to reach the toolbar,
    /// which owns the dialog, and the signal is how.
    #[test]
    fn ctrl_k_opens_the_link_dialog_rather_than_inserting_anything() {
        let code = code_only();
        assert!(code.contains("key.eq_ignore_ascii_case(\"k\")"));
        assert!(code.contains("link_shortcut.set(true)"));
        assert!(
            code.contains("open_link: link_shortcut"),
            "and the toolbar has to be listening for it"
        );
    }

    /// The host owns the source. A component that kept its own copy would drift
    /// from the value the host is about to save, which is the MAPPS-585 defect
    /// in a different place.
    #[test]
    fn the_value_is_the_hosts() {
        let code = code_only();
        assert!(
            !code.contains("use_signal(|| props.value"),
            "the editor must not take a copy of the source"
        );
        assert!(
            code.contains("value: props.value.clone()"),
            "it renders what the host holds"
        );
    }

    /// The label sits above the whole control, not between the toolbar and the
    /// box. `Textarea` draws its own label above the field, which put it under
    /// the toolbar and read as a caption on it; the label is hidden there and
    /// rendered here instead, still as a real `<label for>` so the field keeps
    /// its accessible name and the validation message keeps its field name.
    #[test]
    fn the_label_is_above_the_toolbar_not_between_it_and_the_field() {
        let code = code_only();
        let label = code
            .find("label { r#for:")
            .expect("the editor renders a label");
        let toolbar = code.find("MarkdownToolbar {").expect("the toolbar");
        assert!(label < toolbar, "the label comes first");
        assert!(
            code.contains("label_hidden: true"),
            "and the field does not draw a second one"
        );
    }

    /// MAPPS-585: the value reaches the textarea as an ATTRIBUTE, via the
    /// shared `Textarea`. Reaching for a raw `textarea` here would reintroduce
    /// the child-vs-attribute bug that froze every toolbar button after the
    /// first keystroke.
    #[test]
    fn the_field_is_the_shared_textarea() {
        let code = code_only();
        assert!(code.contains("Textarea {"), "the shared component");
        assert!(
            !code.contains("textarea {"),
            "never a raw element: MAPPS-585 is what that costs"
        );
    }
}

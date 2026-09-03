//! Formatting toolbar for a markdown source field (MAPPS-579).
//!
//! Markdown-first: the source is the single source of truth and every button
//! edits it. That decision is recorded in the issue, but the short version is
//! that a visual mode would need an editor library this app has no interop
//! layer for, and article bodies carry raw `<span style="color:red">` that
//! MAPPS-573 preserves through a deliberately narrow sanitizer filter, so
//! round-tripping through a second representation is a fidelity problem larger
//! than the editor itself.
//!
//! The transforms live in [`crate::utils::md_edit`], away from the DOM, so
//! they are unit-tested on strings. This component is the wiring: read the
//! selection off the real `<textarea>`, apply, write back, restore the
//! selection, and hand the new value to the caller.

use dioxus::prelude::*;

use super::button::{Button, ButtonVariant};
use super::icons::{
    AlignTableIcon, BoldIcon, BulletListIcon, ChecklistIcon, CodeBlockIcon, CodeIcon, HeadingIcon,
    IconSize, ItalicIcon, LinkIcon, NumberedListIcon, PhotoIcon, QuoteIcon, StrikethroughIcon,
    TableIcon,
};
use super::modal::Modal;
use crate::utils::md_edit::{self, Action};

/// Which structural dialog is open, if any.
#[derive(Clone, PartialEq)]
enum Dialog {
    Link,
    Image,
    CodeBlock,
    Table,
}

#[derive(Props, Clone, PartialEq)]
pub struct MarkdownToolbarProps {
    /// `id` of the `<textarea>` this toolbar edits. `Textarea` renders
    /// `id="{name}"`, so this is that field's `name`.
    pub target_id: String,
    /// The current source. The toolbar does not own it; the caller does.
    pub value: String,
    /// Fires with the new source after a transform.
    pub onchange: EventHandler<String>,
    /// Disable every control, matching the form's own write gate.
    #[props(default = false)]
    pub disabled: bool,
    /// MAPPS-579: set by the host when Ctrl+K fires on the body field. The
    /// shortcut lands on the textarea but the dialog belongs here, so the
    /// request arrives as a flag the toolbar consumes and clears.
    #[props(default)]
    pub open_link: Option<Signal<bool>>,
    /// MAPPS-587: where to send a file the author picks, when the host can
    /// store one. `None` leaves the Image dialog URL-only, which is what every
    /// surface other than the KB editor still is: the upload route belongs to
    /// an article, so a toolbar over a ticket note has nowhere to put a file.
    ///
    /// The toolbar deliberately does not upload. It reads the bytes and hands
    /// them over; the host owns the endpoint, the article the file attaches to,
    /// and the decision to create one. That keeps this component ignorant of
    /// articles, which is the only reason it can be shared.
    #[props(default)]
    pub on_file: Option<EventHandler<(String, String, Vec<u8>)>>,
}

/// Length of `value` in UTF-16 code units, which is what the DOM counts.
fn utf16_len(value: &str) -> u32 {
    value.chars().map(|c| c.len_utf16() as u32).sum()
}

/// Apply `action` to the target field and report the new source.
pub fn run_action(target_id: &str, value: &str, action: &Action, onchange: &EventHandler<String>) {
    let (start, end) = crate::platform::dom::textarea_selection(target_id, utf16_len(value));
    let result = md_edit::apply(value, start, end, action);
    let (sel_start, sel_end) = (result.sel_start, result.sel_end);
    onchange.call(result.text);

    // Deferred a frame: the caller's signal rerenders the textarea with the new
    // value, and a selection set before that lands is overwritten by it.
    let id = target_id.to_string();
    spawn(async move {
        crate::platform::timer::sleep_ms(0).await;
        crate::platform::dom::set_textarea_selection(&id, sel_start, sel_end);
    });
}

/// One toolbar button.
///
/// A `title` and an `aria_label` that both name the action AND its shortcut:
/// a tooltip is invisible to a screen reader and an `aria-label` is invisible
/// to a mouse user, so the shortcut has to be in both or one audience never
/// learns it.
#[component]
fn ToolButton(
    label: String,
    shortcut: Option<String>,
    disabled: bool,
    onclick: EventHandler<()>,
    children: Element,
) -> Element {
    let described = match &shortcut {
        Some(k) => format!("{label} ({k})"),
        None => label.clone(),
    };
    rsx! {
        button {
            r#type: "button",
            class: "inline-flex items-center justify-center rounded p-1.5 text-muted hover:bg-surface-2 hover:text-content focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-40 disabled:cursor-not-allowed",
            disabled,
            title: "{described}",
            aria_label: "{described}",
            onclick: move |e: MouseEvent| {
                // The button sits inside the article <form>; without this a
                // click submits it and the author loses the page.
                e.prevent_default();
                onclick.call(());
            },
            {children}
        }
    }
}

/// A thin rule between logical groups, matching the reference's grouping.
#[component]
fn ToolSep() -> Element {
    rsx! {
        span {
            class: "mx-1 h-5 w-px shrink-0 bg-line",
            aria_hidden: "true",
        }
    }
}

#[component]
pub fn MarkdownToolbar(props: MarkdownToolbarProps) -> Element {
    let mut dialog = use_signal(|| None::<Dialog>);
    // Consume the host's Ctrl+K request. Cleared as it is taken, so the dialog
    // does not reopen on every later render.
    if let Some(mut requested) = props.open_link {
        if requested() {
            requested.set(false);
            dialog.set(Some(Dialog::Link));
        }
    }
    // Dialog fields, kept across opens so a mistyped URL is not retyped.
    let mut link_text = use_signal(String::new);
    let mut link_url = use_signal(String::new);
    let mut image_alt = use_signal(String::new);
    let mut image_url = use_signal(String::new);
    let mut code_lang = use_signal(String::new);
    let mut table_rows = use_signal(|| "3".to_string());
    let mut table_cols = use_signal(|| "3".to_string());

    let target = props.target_id.clone();
    let value = props.value.clone();
    let onchange = props.onchange;
    let disabled = props.disabled;

    let fire = move |action: Action| {
        let target = target.clone();
        let value = value.clone();
        move |_: ()| run_action(&target, &value, &action, &onchange)
    };

    rsx! {
        div {
            // `flex-wrap` is the narrow-width strategy: the toolbar wraps onto
            // a second row rather than scrolling actions off the edge, so every
            // control stays reachable without a hidden overflow menu to
            // discover. Chosen over an overflow menu because there are only
            // thirteen actions and they are all one tap.
            class: "flex flex-wrap items-center gap-0.5 rounded-t border border-line border-b-0 bg-surface px-1.5 py-1",
            role: "toolbar",
            aria_label: "Formatting",

            ToolButton {
                label: "Heading".to_string(), shortcut: None, disabled,
                onclick: fire(Action::Heading(2)),
                HeadingIcon { size: IconSize::Small }
            }
            ToolSep {}
            ToolButton {
                label: "Bold".to_string(), shortcut: Some("Ctrl+B".to_string()), disabled,
                onclick: fire(Action::Bold),
                BoldIcon { size: IconSize::Small }
            }
            ToolButton {
                label: "Italic".to_string(), shortcut: Some("Ctrl+I".to_string()), disabled,
                onclick: fire(Action::Italic),
                ItalicIcon { size: IconSize::Small }
            }
            ToolButton {
                label: "Strikethrough".to_string(), shortcut: None, disabled,
                onclick: fire(Action::Strikethrough),
                StrikethroughIcon { size: IconSize::Small }
            }
            ToolSep {}
            ToolButton {
                label: "Quote".to_string(), shortcut: None, disabled,
                onclick: fire(Action::Quote),
                QuoteIcon { size: IconSize::Small }
            }
            ToolButton {
                label: "Inline code".to_string(), shortcut: None, disabled,
                onclick: fire(Action::InlineCode),
                CodeIcon { size: IconSize::Small }
            }
            ToolButton {
                label: "Link".to_string(), shortcut: Some("Ctrl+K".to_string()), disabled,
                onclick: move |_| dialog.set(Some(Dialog::Link)),
                LinkIcon { size: IconSize::Small }
            }
            ToolSep {}
            ToolButton {
                label: "Bulleted list".to_string(), shortcut: None, disabled,
                onclick: fire(Action::BulletList),
                BulletListIcon { size: IconSize::Small }
            }
            ToolButton {
                label: "Numbered list".to_string(), shortcut: None, disabled,
                onclick: fire(Action::NumberedList),
                NumberedListIcon { size: IconSize::Small }
            }
            ToolButton {
                label: "Checklist".to_string(), shortcut: None, disabled,
                onclick: fire(Action::Checklist),
                ChecklistIcon { size: IconSize::Small }
            }
            ToolSep {}
            ToolButton {
                label: "Code block".to_string(), shortcut: None, disabled,
                onclick: move |_| dialog.set(Some(Dialog::CodeBlock)),
                CodeBlockIcon { size: IconSize::Small }
            }
            ToolButton {
                label: "Table".to_string(), shortcut: None, disabled,
                onclick: move |_| dialog.set(Some(Dialog::Table)),
                TableIcon { size: IconSize::Small }
            }
            // MAPPS-600: tidy the table the caret is in. Next to the button
            // that inserts one, because that is where an author looks after
            // typing into it and finding the pipes ragged.
            ToolButton {
                label: "Align table".to_string(), shortcut: None, disabled,
                onclick: fire(Action::FormatTable),
                AlignTableIcon { size: IconSize::Small }
            }
            ToolButton {
                label: "Image".to_string(), shortcut: None, disabled,
                onclick: move |_| dialog.set(Some(Dialog::Image)),
                PhotoIcon { size: IconSize::Small }
            }
        }

        // Structural inserts get a dialog rather than expecting the author to
        // type syntax, matching the reference.
        {
            let open = dialog.read().clone();
            let target = props.target_id.clone();
            let value = props.value.clone();
            let title = match &open {
                Some(Dialog::Link) => "Insert link",
                Some(Dialog::Image) => "Insert image",
                Some(Dialog::CodeBlock) => "Insert code block",
                Some(Dialog::Table) => "Insert table",
                None => "",
            };
            rsx! {
                Modal {
                    open: open.is_some(),
                    title: title.to_string(),
                    size: super::modal::ModalSize::Small,
                    onclose: move |_| dialog.set(None),
                    footer: rsx! {
                        Button {
                            variant: ButtonVariant::Secondary,
                            onclick: move |_| dialog.set(None),
                            "Cancel"
                        }
                        Button {
                            variant: ButtonVariant::Primary,
                            onclick: move |_| {
                                let action = match dialog.read().clone() {
                                    Some(Dialog::Link) => Some(Action::Link {
                                        text: link_text.read().clone(),
                                        url: link_url.read().clone(),
                                    }),
                                    Some(Dialog::Image) => Some(Action::Image {
                                        alt: image_alt.read().clone(),
                                        url: image_url.read().clone(),
                                    }),
                                    Some(Dialog::CodeBlock) => Some(Action::CodeBlock {
                                        lang: code_lang.read().trim().to_string(),
                                    }),
                                    Some(Dialog::Table) => Some(Action::Table {
                                        rows: table_rows.read().parse().unwrap_or(3),
                                        cols: table_cols.read().parse().unwrap_or(3),
                                    }),
                                    None => None,
                                };
                                if let Some(action) = action {
                                    run_action(&target, &value, &action, &onchange);
                                }
                                dialog.set(None);
                            },
                            "Insert"
                        }
                    },
                    div { class: "space-y-3",
                        match open {
                            Some(Dialog::Link) => rsx! {
                                crate::components::Input {
                                    name: "md_link_url",
                                    label: "URL",
                                    placeholder: "https://example.com",
                                    value: link_url.read().clone(),
                                    oninput: move |e: FormEvent| link_url.set(e.value()),
                                }
                                crate::components::Input {
                                    name: "md_link_text",
                                    label: "Text",
                                    help: "Leave blank to use whatever is selected in the body.".to_string(),
                                    value: link_text.read().clone(),
                                    oninput: move |e: FormEvent| link_text.set(e.value()),
                                }
                            },
                            Some(Dialog::Image) => rsx! {
                                // MAPPS-587: a picker, when the host can store
                                // what it picks. Above the URL field because it
                                // is what most people came here to do; the URL
                                // stays for an image that already lives
                                // somewhere.
                                if let Some(on_file) = props.on_file {
                                    crate::components::FileField {
                                        name: "md_image_file",
                                        label: "Upload an image",
                                        accept: crate::utils::image_upload::accept_attribute(),
                                        help: "PNG, JPEG, WebP or GIF, up to 5 MB. It is stored with this article.".to_string(),
                                        onchange: move |evt: FormEvent| {
                                            let Some(file) = evt.files().into_iter().next() else {
                                                return;
                                            };
                                            // Closed on selection: the upload and
                                            // the insert happen in the host, and
                                            // leaving the dialog open over them
                                            // would hide the result.
                                            dialog.set(None);
                                            spawn(async move {
                                                let name = file.name();
                                                let mime = file
                                                    .content_type()
                                                    .unwrap_or_default();
                                                if let Ok(bytes) = file.read_bytes().await {
                                                    on_file.call((name, mime, bytes.to_vec()));
                                                }
                                            });
                                        },
                                    }
                                }
                                crate::components::Input {
                                    name: "md_image_url",
                                    label: "Image URL",
                                    placeholder: "https://example.com/diagram.png",
                                    help: "Or paste a link to an image that already lives somewhere.".to_string(),
                                    value: image_url.read().clone(),
                                    oninput: move |e: FormEvent| image_url.set(e.value()),
                                }
                                crate::components::Input {
                                    name: "md_image_alt",
                                    label: "Alt text",
                                    help: "Describes the image for anyone who cannot see it.".to_string(),
                                    value: image_alt.read().clone(),
                                    oninput: move |e: FormEvent| image_alt.set(e.value()),
                                }
                            },
                            Some(Dialog::CodeBlock) => rsx! {
                                crate::components::Input {
                                    name: "md_code_lang",
                                    label: "Language",
                                    placeholder: "bash, rust, json, yaml, sql…",
                                    help: "Optional. Naming it turns on syntax highlighting.".to_string(),
                                    value: code_lang.read().clone(),
                                    oninput: move |e: FormEvent| code_lang.set(e.value()),
                                }
                            },
                            Some(Dialog::Table) => rsx! {
                                div { class: "grid grid-cols-2 gap-3",
                                    crate::components::Input {
                                        name: "md_table_rows",
                                        label: "Rows",
                                        r#type: "number".to_string(),
                                        value: table_rows.read().clone(),
                                        oninput: move |e: FormEvent| table_rows.set(e.value()),
                                    }
                                    crate::components::Input {
                                        name: "md_table_cols",
                                        label: "Columns",
                                        r#type: "number".to_string(),
                                        value: table_cols.read().clone(),
                                        oninput: move |e: FormEvent| table_cols.set(e.value()),
                                    }
                                }
                            },
                            None => rsx! {},
                        }
                    }
                }
            }
        }
    }
}

/// Map a keydown on the body field to a toolbar action (MAPPS-579).
///
/// `Ctrl` or `Meta`, so the same shortcuts work on macOS and elsewhere without
/// the component asking which platform it is on. Returns `None` for anything
/// else, so the keystroke falls through to the textarea untouched.
pub fn shortcut_action(ctrl_or_meta: bool, key: &str) -> Option<Action> {
    if !ctrl_or_meta {
        return None;
    }
    match key.to_ascii_lowercase().as_str() {
        "b" => Some(Action::Bold),
        "i" => Some(Action::Italic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marks_have_shortcuts_and_nothing_else_does() {
        assert_eq!(shortcut_action(true, "b"), Some(Action::Bold));
        assert_eq!(shortcut_action(true, "B"), Some(Action::Bold));
        assert_eq!(shortcut_action(true, "i"), Some(Action::Italic));
        // Link is handled by the caller, because it opens a dialog rather than
        // applying a transform directly.
        assert_eq!(shortcut_action(true, "k"), None);
        assert_eq!(shortcut_action(true, "z"), None);
    }

    /// Without the modifier a shortcut must not fire, or typing "b" in the body
    /// bolds instead of inserting a letter.
    #[test]
    fn a_bare_letter_is_never_a_shortcut() {
        assert_eq!(shortcut_action(false, "b"), None);
        assert_eq!(shortcut_action(false, "i"), None);
    }
}

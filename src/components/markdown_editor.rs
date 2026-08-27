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

use dioxus::html::HasFileData;
use dioxus::prelude::*;

use super::form::Textarea;
use super::markdown_toolbar::{run_action, shortcut_action, MarkdownToolbar};
use super::mention_autocomplete::MentionAutocomplete;
use crate::utils::mentions::Mention;
use crate::utils::validation::Rule;

/// What the author is looking at in a Markdown editor.
///
/// PMS-939: one state, three values. It replaces a `Write | Preview` tab pair
/// and a separate `Split view: on/off` pill, which were two components in two
/// visual languages at opposite ends of the same row, controlling the same
/// thing. With split on, `Write` stayed underlined and `Preview` stayed
/// offered, so a mode showing both panes still claimed one of them was current.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Write,
    Split,
    Preview,
}

/// The three, in the order they are drawn and cycled.
pub(crate) const VIEW_MODES: [ViewMode; 3] = [ViewMode::Write, ViewMode::Split, ViewMode::Preview];

impl ViewMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Write => "Write",
            Self::Split => "Split",
            Self::Preview => "Preview",
        }
    }

    /// Persisted form. Named rather than an index, so a value written by one
    /// release still means the same thing after the order changes.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Split => "split",
            Self::Preview => "preview",
        }
    }

    /// Anything unrecognised is Write: an author whose stored preference has
    /// gone stale gets the editor, which is what this screen is for.
    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "split" => Self::Split,
            "preview" => Self::Preview,
            _ => Self::Write,
        }
    }

    /// Step `delta` places through the group, wrapping at both ends.
    ///
    /// PMS-939: this wrap-around IS the mode cycling. No global chord is bound
    /// for it, because every plausible one (`Ctrl+Shift+E/M/P/V`) opens a
    /// devtools panel or pastes as plain text in at least one of Chrome and
    /// Firefox, and a shortcut that steals a browser feature is worse than
    /// tabbing to the group and pressing an arrow.
    pub(crate) fn stepped(self, delta: i32) -> Self {
        let len = VIEW_MODES.len() as i32;
        let at = VIEW_MODES.iter().position(|m| *m == self).unwrap_or(0) as i32;
        VIEW_MODES[(((at + delta) % len + len) % len) as usize]
    }

    /// Is the source pane on the page in this mode?
    pub(crate) fn shows_source(self) -> bool {
        self != Self::Preview
    }

    /// Is the preview pane on the page in this mode?
    pub(crate) fn shows_preview(self) -> bool {
        self != Self::Write
    }
}

/// Class for one of the two body panes.
///
/// Hidden rather than unmounted, because a `match` on the mode throws away the
/// caret and the textarea's scroll offset (MAPPS-573).
///
/// PMS-939: the pane is on the page or it is not, with no `lg:` in either
/// branch. MAPPS-584 hid the split control below `lg` on the grounds that a
/// stacked "side by side" is the same page twice. That was right while Split
/// was an extra on top of the tabs; it is wrong now that it is one of three
/// segments, because a segment that does nothing at that width is exactly the
/// dead control this pass is about. Below `lg`, Split stacks the two panes
/// instead, which is what the mode promises.
pub(crate) fn body_pane_class(visible: bool) -> &'static str {
    if visible {
        // A flex column so the field inside can stretch, and `flex-1` so the
        // pane itself stretches when it is the only one (a grid item already
        // stretches; a block child of a fixed-height box does not).
        "min-w-0 flex flex-col flex-1"
    } else {
        "hidden"
    }
}

/// Class for one option in the view switcher.
///
/// PMS-939: the house segmented-control style, taken from the theme picker's
/// Base mode row (`components/theme_picker.rs`) - a `bg-surface-2` track with a
/// raised `bg-surface` pill on the selected option. Selection reads as
/// elevation and weight, not colour alone.
/// Choose a mode and remember it. One place, so the click path and the
/// keyboard path cannot drift over whether the choice is persisted.
pub(crate) fn set_view_mode(view: &mut Signal<ViewMode>, next: ViewMode, pref_key: &str) {
    view.set(next);
    // A host that named no key is not remembering the choice, and writing under
    // an empty key would put one row in storage for every editor that forgot.
    if !pref_key.is_empty() {
        crate::utils::prefs::set_str(pref_key, next.as_str());
    }
}

pub(crate) fn view_mode_class(selected: bool) -> &'static str {
    if selected {
        "px-3 py-1 rounded-md text-sm font-semibold bg-surface text-content shadow-sm"
    } else {
        "px-3 py-1 rounded-md text-sm font-medium text-muted hover:text-content"
    }
}

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
    /// PMS-939: extra classes on the `<textarea>` itself.
    ///
    /// `rows` sets an intrinsic height, which is right for a field in a form
    /// and wrong for the one field that is meant to fill the screen. The KB
    /// editor passes `flex-1 min-h-0` and makes its own wrapper a flex column;
    /// every other host leaves this empty and keeps its rows.
    #[props(default)]
    pub field_class: String,
    /// MAPPS-610: offer `Write | Split | Preview` and a preview pane.
    ///
    /// Off by default, because a host that has nowhere to put a second pane
    /// should not be given one. On, the component owns the switcher, both
    /// panes and the scroll link between them - which is the whole point of it
    /// living here rather than being copied into every page that wants it.
    #[props(default = false)]
    pub views: bool,
    /// MAPPS-610: where the chosen view is remembered. Empty means it is not:
    /// the editor opens in Write every time.
    #[props(default)]
    pub view_pref_key: String,
    /// MAPPS-610: classes on the panel holding the pane(s), which is where a
    /// host states how tall its editor is. Empty leaves the height to `rows`.
    #[props(default)]
    pub panel_class: String,
}

/// MAPPS-610: the preview box's id, derived from the field's.
///
/// `name` is already required to be unique on the page (the toolbar and the
/// mention list both address the field by it), so deriving from it keeps the
/// two ids in step without a second prop for a host to get wrong.
fn preview_id(name: &str) -> String {
    format!("{name}-preview")
}

#[component]
pub fn MarkdownEditor(props: MarkdownEditorProps) -> Element {
    // MAPPS-579: Ctrl+K lands on the textarea but opens a dialog that belongs
    // to the toolbar, so the request travels as a flag the toolbar consumes.
    let mut link_shortcut = use_signal(|| false);

    // MAPPS-610: which pane(s) are up. Seeded from the stored preference when
    // the host names one. Held even when `views` is off, so the panes below can
    // read one rule rather than branching twice.
    let pref_key = props.view_pref_key.clone();
    let mut view = use_signal(move || {
        if pref_key.is_empty() {
            ViewMode::Write
        } else {
            ViewMode::from_str(&crate::utils::prefs::get_str(
                &pref_key,
                ViewMode::Write.as_str(),
            ))
        }
    });
    let showing = if props.views { view() } else { ViewMode::Write };

    let target = props.name.clone();
    let on_change = props.oninput;
    let field_id = props.name.clone();
    let preview_box = preview_id(&props.name);
    let group_id = format!("{}-view", props.name);

    // MAPPS-600: tie the two panes' scrolling together while both are on
    // screen. In an effect rather than at mount because the preview box only
    // exists in split view; re-running is free, `link` marks what it has wired.
    // MAPPS-610: it moved here with the panes, so every host gets it.
    {
        let source = field_id.clone();
        let preview = preview_box.clone();
        use_effect(move || {
            if showing == ViewMode::Split {
                crate::platform::scroll_sync::link(&source, &preview);
            }
        });
    }

    let on_file = props.on_file;

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
            // MAPPS-610: the switcher, between the label and the panes it
            // governs. PMS-939 built it in the KB page; it lives here now so
            // the description and the note editors get the same one rather
            // than a copy each.
            if props.views {
                div { class: "mb-2",
                    div {
                        class: "inline-flex gap-1 rounded-lg bg-surface-2 p-1",
                        role: "radiogroup",
                        aria_label: "Editor view",
                        for option in VIEW_MODES {
                            {
                                let selected = view() == option;
                                let key = props.view_pref_key.clone();
                                let key_for_keys = props.view_pref_key.clone();
                                let group = group_id.clone();
                                rsx! {
                                    button {
                                        key: "{option.as_str()}",
                                        id: "{group}-{option.as_str()}",
                                        r#type: "button",
                                        role: "radio",
                                        class: view_mode_class(selected),
                                        aria_checked: if selected { "true" } else { "false" },
                                        // Roving: only the selected option is in
                                        // the tab order, which is what makes the
                                        // group one stop rather than three.
                                        tabindex: if selected { "0" } else { "-1" },
                                        onclick: move |_| set_view_mode(&mut view, option, &key),
                                        onkeydown: move |e: KeyboardEvent| {
                                            // Selection follows focus, so a step
                                            // both moves and chooses.
                                            let delta = match e.key() {
                                                Key::ArrowRight | Key::ArrowDown => 1,
                                                Key::ArrowLeft | Key::ArrowUp => -1,
                                                _ => 0,
                                            };
                                            if delta == 0 {
                                                return;
                                            }
                                            e.prevent_default();
                                            let next = option.stepped(delta);
                                            set_view_mode(&mut view, next, &key_for_keys);
                                            crate::platform::dom::focus_by_id(&format!(
                                                "{group}-{}",
                                                next.as_str()
                                            ));
                                        },
                                        "{option.label()}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: if showing == ViewMode::Split {
                    // Two equal rows below `lg`, two equal columns from `lg` up.
                    // Explicit rows because the panel may have a definite height
                    // and implicit `auto` rows would size to content and
                    // overflow it.
                    format!(
                        "{} grid gap-4 grid-cols-1 grid-rows-2 lg:grid-cols-2 lg:grid-rows-1",
                        props.panel_class,
                    )
                } else {
                    format!("{} flex flex-col", props.panel_class)
                },
            div {
                class: body_pane_class(showing.shows_source()),
                // MAPPS-587: drop an image anywhere on the write pane.
                // `ondragover` has to prevent the default or the browser never
                // fires `ondrop` and instead navigates away to the file, losing
                // what was written.
                // MAPPS-610: the drop zone moved here with the panes, so any
                // host that can take a file gets it rather than it being one
                // page's layout.
                ondragover: move |e: DragEvent| {
                    if on_file.is_some() {
                        e.prevent_default();
                    }
                },
                ondrop: move |e: DragEvent| {
                    let Some(sink) = on_file else {
                        return;
                    };
                    let files = e.files();
                    if files.is_empty() {
                        return;
                    }
                    // Only when there is a file. A drag of selected text inside
                    // the textarea is an ordinary move and must keep working.
                    e.prevent_default();
                    let Some(file) = files.into_iter().next() else {
                        return;
                    };
                    spawn(async move {
                        let name = file.name();
                        let mime = file.content_type().unwrap_or_default();
                        if let Ok(bytes) = file.read_bytes().await {
                            sink.call((name, mime, bytes.to_vec()));
                        }
                    });
                },
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
                class: format!("rounded-t-none resize-y {}", props.field_class),
                // The wrapper is the flex item, so a field asked to stretch
                // needs the wrapper to stretch with it AND to be a flex column
                // itself, or the field inside it has nothing to stretch against.
                wrapper_class: if props.field_class.is_empty() {
                    String::new()
                } else {
                    format!("flex flex-col {}", props.field_class)
                },
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
            if props.views {
                div {
                    class: body_pane_class(showing.shows_preview()),
                    // The same label treatment as the field's own, because the
                    // two panes are two labelled fields rather than a field and
                    // a caption (PMS-939).
                    p { class: "mb-1 block text-sm font-medium text-content", "Preview" }
                    // MAPPS-579: the SAME component the saved text renders
                    // through, not just the same function. Calling
                    // `render_markdown` directly meant the preview silently
                    // differed from the article in three ways: no resolved
                    // @mentions (MAPPS-578), checkboxes rendered disabled, and a
                    // fixed prose density instead of the reader's. A preview
                    // that is not the document is the bug this exists to avoid.
                    //
                    // MAPPS-600: the scroll box, and the element the source pane
                    // is synced to. A wrapper rather than the `Markdown`
                    // container itself, because `Markdown` owns its own id for
                    // the delegated click listener and giving a shared component
                    // a second identity for one caller is how it starts serving
                    // a page.
                    div {
                        id: "{preview_box}",
                        class: "p-2 border border-line rounded h-full overflow-y-auto",
                        crate::components::Markdown {
                            // No floor of its own: the panel sets the height and
                            // the row stretches both columns to it. Two competing
                            // minimums is what made the tab swap shrink the
                            // document (MAPPS-573).
                            content: props.value.clone(),
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

#[cfg(test)]
mod view_switcher_tests {
    use super::{body_pane_class, preview_id, view_mode_class, ViewMode, VIEW_MODES};

    const SRC: &str = include_str!("markdown_editor.rs");

    fn code_only() -> String {
        let end = SRC
            .find("mod view_switcher_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The whole point of the pass: one control, one selected state, and no
    /// mode in which a visible control means nothing.
    #[test]
    fn exactly_one_mode_is_selected_and_every_mode_shows_a_pane() {
        for mode in VIEW_MODES {
            let selected = VIEW_MODES.iter().filter(|m| **m == mode).count();
            assert_eq!(
                selected,
                1,
                "{:?} selects exactly one segment",
                mode.label()
            );
            assert!(
                mode.shows_source() || mode.shows_preview(),
                "{} must put something on the page",
                mode.label()
            );
        }
        assert!(ViewMode::Split.shows_source() && ViewMode::Split.shows_preview());
        assert!(!ViewMode::Write.shows_preview());
        assert!(!ViewMode::Preview.shows_source());
    }

    /// Arrow keys cycle, wrapping at both ends. This IS the mode-cycling
    /// shortcut: no global chord is bound, because every plausible one collides
    /// with a browser feature in Chrome or Firefox.
    #[test]
    fn the_arrows_cycle_through_the_group_and_wrap() {
        assert!(ViewMode::Write.stepped(1) == ViewMode::Split);
        assert!(ViewMode::Split.stepped(1) == ViewMode::Preview);
        assert!(
            ViewMode::Preview.stepped(1) == ViewMode::Write,
            "wraps forward"
        );
        assert!(
            ViewMode::Write.stepped(-1) == ViewMode::Preview,
            "wraps back"
        );
        assert!(ViewMode::Split.stepped(-1) == ViewMode::Write);
    }

    /// The persisted form is a name, not an index, so reordering the group
    /// later cannot silently change what an author's stored choice means.
    #[test]
    fn the_stored_mode_round_trips_by_name() {
        for mode in VIEW_MODES {
            assert!(ViewMode::from_str(mode.as_str()) == mode);
        }
        assert!(
            ViewMode::from_str("side-by-side") == ViewMode::Write,
            "an unreadable stored value opens the editor, not a blank preview"
        );
        assert!(ViewMode::from_str("") == ViewMode::Write);
    }

    /// Selection reads as elevation and weight, so it survives a viewer who
    /// cannot tell the two colours apart.
    #[test]
    fn the_selected_segment_is_not_marked_by_colour_alone() {
        let on = view_mode_class(true);
        let off = view_mode_class(false);
        assert!(on.contains("shadow-sm") && on.contains("bg-surface"));
        assert!(!off.contains("shadow-sm"));
        assert!(on.contains("font-semibold") && off.contains("font-medium"));
    }

    /// One-of-N is a radio group. Three `aria-pressed` toggles would say three
    /// independent things are on or off, which is what the old tabs-plus-pill
    /// pair actually claimed.
    #[test]
    fn the_group_carries_radio_semantics_and_a_roving_tabindex() {
        let code = code_only();
        assert!(
            code.contains(r#"role: "radiogroup","#),
            "the group says what it is"
        );
        assert!(
            code.contains(r#"role: "radio","#),
            "and each option does too"
        );
        assert!(
            code.contains(r#"aria_checked: if selected { "true" } else { "false" }"#),
            "selection is exposed, not implied by a class"
        );
        assert!(
            code.contains(r#"tabindex: if selected { "0" } else { "-1" }"#),
            "roving tabindex, so the group is one tab stop rather than three"
        );
        assert!(
            !code.contains(r#"aria_pressed: if split()"#),
            "the split toggle is gone, not left alongside the group"
        );
    }

    /// Split has to work at every width now that it is a segment. Hiding it
    /// below `lg`, which is what MAPPS-584 did to the old toggle, would leave a
    /// segment that does nothing - the exact defect this pass is about.
    #[test]
    fn split_stacks_below_lg_rather_than_being_unavailable() {
        let code = code_only();
        assert!(
            code.contains("grid gap-4 grid-cols-1 grid-rows-2 lg:grid-cols-2 lg:grid-rows-1"),
            "two equal rows below lg, two equal columns above"
        );
        assert!(
            !body_pane_class(true).contains("lg:") && !body_pane_class(false).contains("lg:"),
            "a pane's visibility no longer depends on the width"
        );
    }

    /// The two panes are labelled the same way, because they are two labelled
    /// fields rather than a field and a caption.
    #[test]
    fn both_panes_carry_the_same_label_treatment() {
        let code = code_only();
        assert!(
            code.contains(
                r#"p { class: "mb-1 block text-sm font-medium text-content", "Preview" }"#
            ),
            "the preview label matches Body (Markdown)"
        );
        assert!(
            !code.contains(r#"text-xs font-medium text-muted", "Preview""#),
            "and the old caption styling is gone"
        );
    }

    /// The field stretches to the panel instead of standing at 18 rows, and the
    /// stretch has to reach it through every wrapper in between.
    #[test]
    fn the_body_field_stretches_to_the_panel() {
        let code = code_only();
        assert!(
            code.contains("wrapper_class:"),
            "the stretch reaches the textarea's own wrapper, which is the flex item"
        );
        const KB: &str = include_str!("../pages/knowledge_base.rs");
        assert!(
            KB.contains(r#"field_class: "flex-1 min-h-0".to_string()"#),
            "and the KB body, the one editor meant to fill the screen, asks for it"
        );
    }

    /// MAPPS-610: the preview box's id is derived from the field's, so a page
    /// with several editors on it cannot end up with two boxes answering to one
    /// id and a scroll link that ties the wrong pair together.
    #[test]
    fn the_preview_box_is_named_after_its_field() {
        assert_eq!(preview_id("content"), "content-preview");
        assert_ne!(preview_id("content"), preview_id("edit-description"));
        let code = code_only();
        assert!(
            code.contains(r#"id: "{preview_box}","#),
            "the box carries the derived id"
        );
        assert!(
            code.contains("crate::platform::scroll_sync::link(&source, &preview);"),
            "and the sync links that same pair"
        );
    }

    /// MAPPS-600, moved here with the panes. The scrolling box is the wrapper,
    /// not the `Markdown` container: the shared renderer owns its own id for
    /// the delegated click listener, and giving it a second identity for one
    /// caller's benefit is how a component starts serving a page.
    #[test]
    fn the_preview_scrolls_in_a_wrapper_this_component_owns() {
        let code = code_only();
        let wrapper = code
            .find(r#"id: "{preview_box}","#)
            .expect("the preview wrapper");
        let window = &code[wrapper..code.len().min(wrapper + 700)];
        assert!(
            window.contains("overflow-y-auto"),
            "the wrapper is the scroll box: {window}"
        );
        assert!(
            window.contains("crate::components::Markdown {"),
            "and the renderer sits inside it: {window}"
        );
    }

    /// MAPPS-610: the switcher exists in one place. Copying it into the three
    /// other editors is what this move avoids.
    #[test]
    fn no_page_renders_its_own_switcher() {
        for (name, src) in [
            ("knowledge_base", include_str!("../pages/knowledge_base.rs")),
            ("tickets", include_str!("../pages/tickets.rs")),
        ] {
            assert!(
                !src.contains(r#"role: "radiogroup","#),
                "{name} must reach the switcher through MarkdownEditor, not draw one"
            );
        }
    }
}

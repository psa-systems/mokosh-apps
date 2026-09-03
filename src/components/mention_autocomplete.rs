//! Complete an `@mention` while typing (MAPPS-580).
//!
//! Mention *rendering* has worked since MAPPS-578: a saved `@handle` resolves
//! to a person and renders as a chip. Typing one was still guesswork, because
//! the author had to know the handle before they typed it.
//!
//! The trigger and the fragment logic live in [`crate::utils::mentions`], pure
//! and unit-tested, for the same reason the toolbar's transforms live in
//! `md_edit`: they are the part that can be wrong in ways a screenshot does not
//! show. This component is the popover and the keyboard.
//!
//! Two properties are worth stating because they are the ones that make an
//! autocomplete trustworthy rather than annoying:
//!
//! - It opens only where the RENDERER would read a mention, sharing
//!   `mentions::opens_a_handle`. Offering to complete something that will
//!   render as plain text teaches the author the wrong thing.
//! - Dismissing never rewrites text. What the author typed stays exactly as
//!   typed unless they accept a suggestion.

use dioxus::prelude::*;

use crate::hooks::dropdown_nav::{use_dropdown_nav, NavAction};
use crate::utils::mentions::{self, ActiveMention, Mention};

#[derive(Props, Clone, PartialEq)]
pub struct MentionAutocompleteProps {
    /// `id` of the `<textarea>` being typed in, so the caret can be read.
    pub target_id: String,
    /// The current source. Owned by the caller.
    pub value: String,
    /// The staff directory to match against. Empty disables the whole feature,
    /// which is the correct degrade: `GET /auth/directory` can fail, and a
    /// mention the author types by hand still resolves at render time.
    pub people: Vec<Mention>,
    /// Fires with the new source and the caret offset after accepting.
    pub onaccept: EventHandler<(String, u32)>,
}

/// Read the caret, in UTF-16 units, from the field being typed in.
fn caret_of(target_id: &str, value: &str) -> u32 {
    let end = value.chars().map(|c| c.len_utf16() as u32).sum();
    let (_, caret) = crate::platform::dom::textarea_selection(target_id, end);
    caret
}

#[component]
pub fn MentionAutocomplete(props: MentionAutocompleteProps) -> Element {
    let mut nav = use_dropdown_nav("mention-ac");
    // The fragment being completed, recomputed on every input by the host.
    let active: Option<ActiveMention> = if props.people.is_empty() {
        None
    } else {
        mentions::active_mention(&props.value, caret_of(&props.target_id, &props.value))
    };

    let rows: Vec<Mention> = match &active {
        Some(a) => mentions::matches(&a.fragment, &props.people)
            .into_iter()
            .cloned()
            .collect(),
        None => Vec::new(),
    };

    // Nothing to offer is not an empty list, it is no list. A popover reading
    // "no matches" over every unrecognised word would be noise.
    if active.is_none() || rows.is_empty() {
        if nav.is_open() {
            nav.close();
        }
        return rsx! {};
    }
    if !nav.is_open() {
        nav.open_fresh();
    }

    let value = props.value.clone();
    let onaccept = props.onaccept;
    let active_for_click = active.clone();
    let rows_for_click = rows.clone();

    rsx! {
        div { class: "relative",
            div {
                id: nav.panel_id(),
                role: "listbox",
                // Anchored under the field rather than at the caret. A
                // caret-position popover needs the field's own text metrics,
                // which means measuring a mirror element; that is a real
                // improvement and is noted on MAPPS-580 rather than half-built
                // here. Under the field, the list is always adjacent to what
                // is being typed on a body this size.
                class: "dropdown-panel absolute z-20 left-0 right-0 mt-1 max-h-60 overflow-y-auto",
                aria_label: "People you can mention",
                for (i , person) in rows_for_click.iter().enumerate() {
                    {
                        let handle = person
                            .handles()
                            .first()
                            .cloned()
                            .unwrap_or_else(|| person.display.clone());
                        let value = value.clone();
                        let active = active_for_click.clone();
                        rsx! {
                            button {
                                key: "{person.id}",
                                id: nav.row_id(i),
                                r#type: "button",
                                role: "option",
                                aria_selected: nav.row_selected(i),
                                class: nav.row_class(i, "block w-full px-3 py-2 text-left text-sm"),
                                onclick: move |e: MouseEvent| {
                                    e.prevent_default();
                                    let Some(a) = active.clone() else { return };
                                    let (text, caret) = mentions::accept(&value, &a, &handle);
                                    onaccept.call((text, caret));
                                },
                                span { class: "font-medium text-content", "{person.display}" }
                                span { class: "ml-2 text-xs text-muted", "@{handle}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Handle a keydown on the body field while the list is open.
///
/// Returns `true` when the key was consumed, so the caller leaves the textarea
/// alone. Escape and a click elsewhere close without touching the text: an
/// autocomplete that rewrites what you typed when you dismiss it is worse than
/// one that does nothing.
pub fn handle_keydown(
    nav: &mut crate::hooks::dropdown_nav::DropdownNav,
    e: &KeyboardEvent,
    rows: &[Mention],
    value: &str,
    active: Option<&ActiveMention>,
    onaccept: &EventHandler<(String, u32)>,
) -> bool {
    let Some(active) = active else {
        return false;
    };
    if rows.is_empty() {
        return false;
    }
    let mut accepted = false;
    let action = nav.keydown(e, rows.len(), |index| {
        if let Some(person) = rows.get(index) {
            let handle = person
                .handles()
                .first()
                .cloned()
                .unwrap_or_else(|| person.display.clone());
            let (text, caret) = mentions::accept(value, active, &handle);
            onaccept.call((text, caret));
            accepted = true;
        }
    });
    accepted || !matches!(action, NavAction::Ignore)
}

#[cfg(test)]
mod tests {
    /// MAPPS-580: the trigger rule is shared with the renderer, not
    /// reimplemented here. If this component grew its own idea of what an `@`
    /// means, the editor would offer completions the renderer will not resolve.
    #[test]
    fn the_trigger_comes_from_the_shared_rule() {
        const SRC: &str = include_str!("mention_autocomplete.rs");
        let end = SRC.find("mod tests").expect("this module is in this file");
        let code = &SRC[..end];
        assert!(
            code.contains("mentions::active_mention("),
            "the trigger must come from utils::mentions"
        );
        assert!(
            !code.contains("== '@'"),
            "and must not be reimplemented by looking for an @ here"
        );
    }

    /// Mentions resolve against the PMS-921 staff directory, everywhere.
    ///
    /// `/auth/users` is `RequireManager`, so reading the roster from there
    /// leaves a Technician with a 403 and no autocomplete, which is the gap
    /// PMS-921 was filed to close for the renderer. It is also the only source
    /// that carries `handle`, which is what resolution matches on.
    ///
    /// Pinned because this exact collision already happened once: MAPPS-580 was
    /// written against `/auth/users` on a branch that predated the rename, and
    /// merging both broke `main`.
    ///
    /// MAPPS-592 turned it from a rule three files had to keep into one they
    /// cannot break: there is a single fetch now, in `hooks::mentions`, and
    /// what this guards is that nobody grows another.
    #[test]
    fn every_mention_source_is_the_staff_directory() {
        const HOOK: &str = include_str!("../hooks/mentions.rs");
        assert!(
            HOOK.contains("\"/auth/directory\""),
            "the one fetch reads the staff directory"
        );
        for (what, src) in [
            ("the editor", include_str!("../pages/knowledge_base.rs")),
            ("the renderer", include_str!("markdown.rs")),
            (
                "the ticket description",
                include_str!("../pages/tickets.rs"),
            ),
        ] {
            assert!(
                !src.contains("get_all_authed::<Row>(\"/auth")
                    && !src.contains("DirectoryEntry>(\"/auth")
                    && !src.contains("DirectoryUser>(\"/auth"),
                "{what} must read the directory through `use_mention_directory`, \
                 not fetch its own: three copies is how one of them came to \
                 handle a nameless row differently from the others"
            );
        }
    }

    /// Dismissing must not rewrite text. The only call that changes the source
    /// is the one behind an explicit accept.
    #[test]
    fn only_accepting_changes_the_text() {
        const SRC: &str = include_str!("mention_autocomplete.rs");
        let end = SRC.find("mod tests").expect("this module is in this file");
        let code = &SRC[..end];
        assert_eq!(
            code.matches("mentions::accept(").count(),
            2,
            "exactly two accept paths, the click and the key; anything else \
             rewriting the source would be a dismissal that edits"
        );
        assert_eq!(
            code.matches("onaccept.call(").count(),
            2,
            "and each reports through the same handler"
        );
    }
}

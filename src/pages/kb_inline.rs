//! Inline comments on the rendered article (MAPPS-744).
//!
//! A comment anchored to a passage: the reader selects text, presses
//! "Comment on selection", and the anchor is captured from the RENDERED
//! text (`utils::anchor::capture`) and sent with the comment (PMS-1130).
//! On every render of the body, every anchored root is resolved against the
//! rendered text again (`utils::anchor::resolve`), the ranges are split at
//! every overlap (`split_overlaps`) and wrapped in `<mark>` elements after
//! sanitizing (`platform::anchor_dom`), and a marker in the left gutter says
//! how many threads sit at each passage. An anchor that no longer resolves
//! is reported to the stream as an orphan: the comment stays, labelled,
//! quoting what it was attached to. Clicking a mark, its marker, or pressing
//! Enter on a focused mark opens the threads at that passage.
//!
//! What this module owns is the orchestration; the resolver is pure and
//! tested on strings, the DOM work lives in the platform layer.

use dioxus::prelude::*;

use crate::components::{Button, ButtonSize, ButtonVariant};
use crate::modules::kb::KbComment;
use crate::pages::knowledge_base::KB_ARTICLE_BODY_ID;
use crate::platform::anchor_dom::{self, MarkPosition, MarkSpec};
use crate::utils::anchor::{self, Anchor};

/// Resolve every anchored, live root against `text`: the fragments to mark,
/// in document order, and the roots that no longer resolve.
pub fn place(text: &str, roots: &[KbComment]) -> (Vec<MarkSpec>, Vec<uuid::Uuid>) {
    let mut ranges: Vec<(String, std::ops::Range<usize>)> = Vec::new();
    let mut orphans: Vec<uuid::Uuid> = Vec::new();
    for c in roots.iter().filter(|c| !c.deleted && c.parent_id.is_none()) {
        let Some(a) = c.anchor.as_ref().and_then(Anchor::from_json) else {
            continue;
        };
        match anchor::resolve(text, &a) {
            Some(r) => ranges.push((c.id.to_string(), r)),
            None => orphans.push(c.id),
        }
    }
    ranges.sort_by_key(|(_, r)| r.start);
    let specs = anchor::split_overlaps(&ranges)
        .into_iter()
        .map(|f| MarkSpec {
            start: f.range.start,
            end: f.range.end,
            ids: f.owners,
        })
        .collect();
    (specs, orphans)
}

/// MAPPS-745: the quotes of the anchored roots that resolve in the article
/// as saved (`before`) and no longer resolve in the unsaved body (`after`).
/// What the editor warns about on save. A root already orphaned before the
/// edit is not this edit's doing and is left out.
pub fn orphaned_by_edit(before: &str, after: &str, roots: &[KbComment]) -> Vec<String> {
    let before_text = crate::utils::markdown::rendered_text(before);
    let after_text = crate::utils::markdown::rendered_text(after);
    roots
        .iter()
        .filter(|c| !c.deleted && c.parent_id.is_none())
        .filter_map(|c| c.anchor.as_ref().and_then(Anchor::from_json))
        .filter(|a| {
            anchor::resolve(&before_text, a).is_some() && anchor::resolve(&after_text, a).is_none()
        })
        .map(|a| a.exact)
        .collect()
}

/// A stable fingerprint of what the placement depends on, so the effect
/// re-runs exactly when the body or an anchor changed.
fn placement_key(content: &str, roots: &[KbComment]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    for c in roots {
        c.id.hash(&mut h);
        c.deleted.hash(&mut h);
        c.anchor.as_ref().map(|a| a.to_string()).hash(&mut h);
    }
    h.finish()
}

#[component]
pub fn InlineComments(
    article_id: String,
    content: String,
    comments_resource: Resource<Option<Vec<KbComment>>>,
    orphans: Signal<Vec<uuid::Uuid>>,
    focus_thread: Signal<Option<uuid::Uuid>>,
    on_changed: EventHandler<()>,
) -> Element {
    let roots: Vec<KbComment> = comments_resource
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default();
    let mut markers = use_signal(Vec::<MarkPosition>::new);
    let mut open_at = use_signal(|| None::<Vec<String>>);
    let mut pending = use_signal(|| None::<Anchor>);
    let mut hint = use_signal(String::new);
    let can_mutate = crate::hooks::use_can_mutate();
    let mut orphans = orphans;
    let mention_directory = crate::hooks::use_mention_directory(true);
    let people = crate::hooks::mention_people(&mention_directory);

    // Re-place on every change of the body or of the anchors: read the
    // rendered text, unwrap yesterday's marks, resolve, wrap, measure.
    #[cfg(feature = "app")]
    {
        let key = placement_key(&content, &roots);
        let roots_for_effect = roots.clone();
        use_effect(use_reactive!(|key| {
            let _ = key;
            let roots = roots_for_effect.clone();
            spawn(async move {
                let Some(text) = anchor_dom::text_of(KB_ARTICLE_BODY_ID).await else {
                    return;
                };
                anchor_dom::clear_highlights(KB_ARTICLE_BODY_ID);
                let (specs, orphan_ids) = place(&text, &roots);
                anchor_dom::apply_highlights(KB_ARTICLE_BODY_ID, &specs);
                orphans.set(orphan_ids);
                markers.set(anchor_dom::mark_positions(KB_ARTICLE_BODY_ID).await);
            });
        }));
        let on_open = EventHandler::new(move |ids: Vec<String>| open_at.set(Some(ids)));
        use_effect(move || anchor_dom::watch_mark_activation(KB_ARTICLE_BODY_ID, on_open));
    }

    let open_ids: Vec<KbComment> = open_at()
        .map(|ids| {
            roots
                .iter()
                .filter(|c| ids.iter().any(|id| *id == c.id.to_string()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    rsx! {
        // The gutter: one marker per passage, at the mark's own height.
        div { class: "pointer-events-none absolute left-0 top-0 h-full w-7", "aria-label": "Inline comment markers",
            for m in markers.read().iter().cloned() {
                {
                    let n = m.ids.len();
                    let label = if n == 1 { "1 inline comment".to_string() } else { format!("{n} inline comments") };
                    let ids = m.ids.clone();
                    rsx! {
                        button {
                            key: "{m.ids.join(\",\")}",
                            r#type: "button",
                            class: "pointer-events-auto absolute left-0 flex h-5 min-w-5 items-center justify-center rounded-full bg-amber-300 px-1 text-[10px] font-semibold text-amber-950 ring-2 ring-surface hover:bg-amber-400 focus:outline-none focus:ring-accent dark:bg-amber-500 dark:text-amber-950",
                            style: "top: {m.top}px",
                            title: "{label}",
                            "aria-label": "{label}",
                            onclick: move |_| open_at.set(Some(ids.clone())),
                            "{n}"
                        }
                    }
                }
            }
        }

        div { class: "mt-4 space-y-3",
            // Select-to-comment. `mousedown` is prevented so the click does
            // not collapse the selection it is about to capture.
            div { class: "flex items-center gap-3",
                // The wrapper takes the mousedown: the default action of a
                // mousedown on a button collapses the selection, and it is
                // decided after the event has bubbled here.
                span { onmousedown: move |e: MouseEvent| e.prevent_default(),
                Button {
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Small,
                    disabled: !can_mutate,
                    title: (!can_mutate).then(|| "Can't comment while the server is unreachable".to_string()),
                    onclick: move |_| {
                        spawn(async move {
                            let Some(range) = anchor_dom::selection_in(KB_ARTICLE_BODY_ID).await else {
                                hint.set("Select some of the article text first, then press this.".to_string());
                                return;
                            };
                            let Some(text) = anchor_dom::text_of(KB_ARTICLE_BODY_ID).await else {
                                return;
                            };
                            match Anchor::capture(&text, range) {
                                Some(a) => {
                                    hint.set(String::new());
                                    pending.set(Some(a));
                                    anchor_dom::clear_selection();
                                }
                                None => hint.set("Select some of the article text first, then press this.".to_string()),
                            }
                        });
                    },
                    "Comment on selection"
                }
                }
                if !hint().is_empty() {
                    span { class: "text-xs text-muted", "{hint}" }
                }
            }

            // The threads at the activated passage.
            if let Some(ids) = open_at() {
                div { class: "rounded-md border border-line bg-surface p-3 shadow-sm", role: "dialog", "aria-label": "Comments on this passage",
                    div { class: "flex items-center justify-between",
                        p { class: "text-sm font-medium text-content",
                            if open_ids.len() == 1 { "Comment on this passage" } else { "Comments on this passage" }
                        }
                        Button { variant: ButtonVariant::Ghost, size: ButtonSize::Small, onclick: move |_| open_at.set(None), "Close" }
                    }
                    if open_ids.is_empty() {
                        p { class: "mt-1 text-xs text-subtle", "These comments are no longer in the list ({ids.len()})." }
                    }
                    ul { class: "mt-2 divide-y divide-line",
                        for c in open_ids.iter() {
                            {
                                let excerpt: String = c.body.chars().take(120).collect();
                                let id = c.id;
                                rsx! {
                                    li { key: "{c.id}", class: "flex items-start justify-between gap-3 py-2 text-sm",
                                        div { class: "min-w-0",
                                            span { class: "font-medium text-content", "{c.author_name}" }
                                            p { class: "truncate text-muted", "{excerpt}" }
                                        }
                                        Button {
                                            variant: ButtonVariant::Link,
                                            size: ButtonSize::Small,
                                            onclick: move |_| {
                                                focus_thread.set(Some(id));
                                                open_at.set(None);
                                                crate::platform::dom::scroll_into_view(&format!("kb-comment-{id}"), true);
                                            },
                                            "Open thread"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // The composer for a captured passage.
            if let Some(a) = pending() {
                div { class: "rounded-md border border-accent/40 bg-surface-2 p-3",
                    p { class: "text-xs text-muted", "Commenting on:" }
                    blockquote { class: "mt-1 border-l-2 border-amber-400 pl-2 text-sm italic text-content", "\u{201c}{a.exact}\u{201d}" }
                    div { class: "mt-3",
                        crate::pages::kb_activity::CommentComposer {
                            article_id: article_id.clone(),
                            parent_id: None,
                            anchor: Some(a.to_json()),
                            author_name: String::new(),
                            author_avatar: None,
                            can_mutate,
                            people: people.clone(),
                            placeholder: "Comment on this passage, @mention people".to_string(),
                            submit_label: "Comment".to_string(),
                            on_posted: move |_| {
                                pending.set(None);
                                on_changed.call(());
                            },
                            on_cancel: Some(EventHandler::new(move |_: ()| pending.set(None))),
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(exact: &str, prefix: &str, suffix: &str, deleted: bool) -> KbComment {
        KbComment {
            id: uuid::Uuid::new_v4(),
            article_id: uuid::Uuid::new_v4(),
            parent_id: None,
            author_id: uuid::Uuid::new_v4(),
            author_name: "Ada".into(),
            author_avatar_url: None,
            body: "hi".into(),
            anchor: Some(serde_json::json!({
                "type": "TextQuoteSelector", "exact": exact, "prefix": prefix, "suffix": suffix
            })),
            anchor_version: Some(1),
            created_at: chrono::Utc::now(),
            edited_at: None,
            resolved_at: None,
            resolved_by_name: None,
            deleted,
            replies: Vec::new(),
        }
    }

    /// Two anchors that overlap become three fragments, the middle one owned
    /// by both; a quote that is gone is an orphan; a deleted comment is
    /// neither marked nor reported.
    #[test]
    fn placement_marks_overlaps_and_reports_orphans() {
        let text = "Restart the router before you call the customer back.";
        let a = root("Restart the router", "", "before", false);
        let b = root("router before you call", "the", "the", false);
        let gone = root("Escalate to the vendor", "", "", false);
        let deleted = root("call the customer", "", "", true);
        let (specs, orphans) = place(text, &[a.clone(), b.clone(), gone.clone(), deleted.clone()]);
        let owners: Vec<Vec<String>> = specs.iter().map(|s| s.ids.clone()).collect();
        assert_eq!(
            owners,
            vec![
                vec![a.id.to_string()],
                vec![a.id.to_string(), b.id.to_string()],
                vec![b.id.to_string()],
            ],
            "{specs:?}"
        );
        assert_eq!(specs[0].start, 0);
        assert_eq!(specs[2].end, text.find("call").unwrap() + "call".len());
        assert_eq!(orphans, vec![gone.id]);
    }

    #[test]
    fn the_placement_key_moves_with_the_body_or_an_anchor_and_not_with_a_body_edit_of_a_comment() {
        let a = root("x", "", "", false);
        let k1 = placement_key("body", std::slice::from_ref(&a));
        assert_eq!(k1, placement_key("body", std::slice::from_ref(&a)));
        assert_ne!(k1, placement_key("body changed", std::slice::from_ref(&a)));
        let mut b = a.clone();
        b.body = "a different comment text".into();
        assert_eq!(
            k1,
            placement_key("body", &[b]),
            "the comment's text is not a placement input"
        );
        let mut c = a.clone();
        c.deleted = true;
        assert_ne!(k1, placement_key("body", &[c]));
    }
}

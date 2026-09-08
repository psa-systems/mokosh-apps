//! Inline comments in the rendered article: the DOM half (MAPPS-744).
//!
//! `utils::anchor` decides WHERE a quote is, as `char` ranges over the
//! rendered text. This module is everything that touches the document to
//! act on that: reading the rendered text, wrapping ranges in `<mark>`
//! elements and unwrapping them again, reading the reader's selection as a
//! `char` range, reporting where the marks sit for the margin, and telling
//! Rust when a mark is activated.
//!
//! The marks are applied AFTER the sanitizer has run and the HTML is in the
//! document, by splitting text nodes at fragment boundaries and wrapping the
//! pieces: no attribute has to survive the renderer, the allowlist does not
//! widen, and the stored article is untouched. `clear_highlights` unwraps
//! and normalizes, so applying is idempotent across renders.
//!
//! Offsets are `char` indices into the container's `textContent`, on both
//! hosts and in the resolver; a text node's `splitText` takes UTF-16 units,
//! so the conversion happens at the one place a node is cut.
//!
//! Both hosts, the `dom.rs` shape: the browser build uses web-sys; the
//! desktop injects the same logic as a script and reads answers back over
//! the `eval` channel, so every read here is `async`.

/// The classes a mark carries, by how many threads cover it: a dotted
/// underline plus a tint (never colour alone), stepped thicker as anchors
/// stack. Kept as literals so Tailwind sees them in source.
pub fn mark_class(depth: usize) -> &'static str {
    match depth {
        0 | 1 => "kb-mark cursor-pointer rounded-sm bg-amber-200/60 underline decoration-amber-700 decoration-dotted decoration-2 underline-offset-2 focus:outline-none focus:ring-2 focus:ring-accent dark:bg-amber-500/30 dark:decoration-amber-300",
        2 => "kb-mark cursor-pointer rounded-sm bg-amber-300/70 underline decoration-amber-800 decoration-dotted decoration-4 underline-offset-4 focus:outline-none focus:ring-2 focus:ring-accent dark:bg-amber-500/40 dark:decoration-amber-200",
        _ => "kb-mark cursor-pointer rounded-sm bg-amber-400/70 underline decoration-amber-900 decoration-double decoration-4 underline-offset-4 focus:outline-none focus:ring-2 focus:ring-accent dark:bg-amber-500/50 dark:decoration-amber-100",
    }
}

/// One fragment to wrap: `char` offsets into the container's text, the
/// comment ids that cover it (comma-joined on the element), and how many.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkSpec {
    pub start: usize,
    pub end: usize,
    pub ids: Vec<String>,
}

/// Where a mark sits, for the margin: its owners and its top relative to
/// the container, in CSS pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkPosition {
    pub ids: Vec<String>,
    pub top: f64,
}

fn aria_label(n: usize) -> String {
    if n == 1 {
        "Inline comment".to_string()
    } else {
        format!("{n} inline comments")
    }
}

// ---------------------------------------------------------------------------
// browser
// ---------------------------------------------------------------------------

/// `NodeFilter.SHOW_TEXT`; web-sys binds the dictionary, not the constants.
#[cfg(target_arch = "wasm32")]
const SHOW_TEXT: u32 = 0x4;

#[cfg(target_arch = "wasm32")]
fn container(id: &str) -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
}

/// The container's rendered text: what the resolver runs over.
#[cfg(target_arch = "wasm32")]
pub async fn text_of(container_id: &str) -> Option<String> {
    container(container_id).and_then(|c| c.text_content())
}

/// Unwrap every mark in the container and merge the text back, so the
/// next apply starts from the sanitizer's own text nodes.
#[cfg(target_arch = "wasm32")]
pub fn clear_highlights(container_id: &str) {
    let Some(root) = container(container_id) else {
        return;
    };
    let Ok(marks) = root.query_selector_all("mark[data-comment]") else {
        return;
    };
    for i in 0..marks.length() {
        let Some(mark) = marks.get(i) else { continue };
        let Some(parent) = mark.parent_node() else {
            continue;
        };
        while let Some(child) = mark.first_child() {
            let _ = parent.insert_before(&child, Some(&mark));
        }
        let _ = parent.remove_child(&mark);
    }
    root.normalize();
}

/// Wrap each fragment in a `<mark>`. `specs` must be non-overlapping and in
/// document order, which `utils::anchor::split_overlaps` guarantees.
#[cfg(target_arch = "wasm32")]
pub fn apply_highlights(container_id: &str, specs: &[MarkSpec]) {
    use wasm_bindgen::JsCast;
    let Some(root) = container(container_id) else {
        return;
    };
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(walker) = document.create_tree_walker_with_what_to_show(&root, SHOW_TEXT) else {
        return;
    };
    // Snapshot the text nodes with their char offsets before any cut.
    let mut nodes: Vec<(web_sys::Text, usize, usize)> = Vec::new();
    let mut pos = 0usize;
    while let Ok(Some(node)) = walker.next_node() {
        let Ok(text) = node.dyn_into::<web_sys::Text>() else {
            continue;
        };
        let len = text.data().chars().count();
        nodes.push((text, pos, pos + len));
        pos += len;
    }
    for (text, node_start, node_end) in nodes {
        let mut current = text;
        let mut current_start = node_start;
        for spec in specs
            .iter()
            .filter(|s| s.start < node_end && s.end > node_start)
        {
            let piece_start = spec.start.max(node_start);
            let piece_end = spec.end.min(node_end);
            if piece_start > current_start {
                let cut = utf16_len(&current.data(), piece_start - current_start);
                let Ok(rest) = current.split_text(cut) else {
                    return;
                };
                current = rest;
                current_start = piece_start;
            }
            let cut = utf16_len(&current.data(), piece_end - current_start);
            let Ok(after) = current.split_text(cut) else {
                return;
            };
            let Ok(mark) = document.create_element("mark") else {
                return;
            };
            let _ = mark.set_attribute("data-comment", &spec.ids.join(","));
            let _ = mark.set_attribute("class", mark_class(spec.ids.len()));
            let _ = mark.set_attribute("tabindex", "0");
            let _ = mark.set_attribute("role", "button");
            let _ = mark.set_attribute("aria-label", &aria_label(spec.ids.len()));
            if let Some(parent) = current.parent_node() {
                let _ = parent.insert_before(&mark, Some(&current));
                let _ = mark.append_child(&current);
            }
            current = after;
            current_start = piece_end;
        }
    }
}

/// UTF-16 units spanned by the first `chars` characters of `s`.
#[cfg(any(target_arch = "wasm32", test))]
fn utf16_len(s: &str, chars: usize) -> u32 {
    s.chars().take(chars).map(|c| c.len_utf16() as u32).sum()
}

/// Every mark's owners and its top relative to the container, one entry
/// per distinct owner set (the first fragment of a stacked anchor).
#[cfg(target_arch = "wasm32")]
pub async fn mark_positions(container_id: &str) -> Vec<MarkPosition> {
    use wasm_bindgen::JsCast;
    let Some(root) = container(container_id) else {
        return Vec::new();
    };
    let root_top = root.get_bounding_client_rect().top();
    let Ok(marks) = root.query_selector_all("mark[data-comment]") else {
        return Vec::new();
    };
    let mut out: Vec<MarkPosition> = Vec::new();
    for i in 0..marks.length() {
        let Some(el) = marks
            .get(i)
            .and_then(|n| n.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        let ids: Vec<String> = el
            .get_attribute("data-comment")
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if out.iter().any(|m| m.ids == ids) {
            continue;
        }
        let top = el.get_bounding_client_rect().top() - root_top;
        out.push(MarkPosition { ids, top });
    }
    out
}

/// The reader's current selection inside the container, as a `char` range
/// into its text; `None` when there is none or it reaches outside.
#[cfg(target_arch = "wasm32")]
pub async fn selection_in(container_id: &str) -> Option<std::ops::Range<usize>> {
    let root = container(container_id)?;
    let window = web_sys::window()?;
    let document = window.document()?;
    let selection = window.get_selection().ok().flatten()?;
    if selection.range_count() == 0 || selection.is_collapsed() {
        return None;
    }
    let range = selection.get_range_at(0).ok()?;
    let common = range.common_ancestor_container().ok()?;
    if !root.contains(Some(&common)) {
        return None;
    }
    let before = document.create_range().ok()?;
    before.select_node_contents(&root).ok()?;
    before
        .set_end(&range.start_container().ok()?, range.start_offset().ok()?)
        .ok()?;
    let start = String::from(before.to_string()).chars().count();
    let len = String::from(range.to_string()).chars().count();
    (len > 0).then_some(start..start + len)
}

/// Drop the reader's selection once it has become a comment.
#[cfg(target_arch = "wasm32")]
pub fn clear_selection() {
    if let Some(selection) = web_sys::window().and_then(|w| w.get_selection().ok().flatten()) {
        let _ = selection.remove_all_ranges();
    }
}

/// Report the owners of any mark the reader activates, by click or by
/// Enter / Space while it is focused. One delegated listener per
/// container, installed once (a flag on the element says so). The
/// callback is an `EventHandler` for the MAPPS-586 reason.
#[cfg(target_arch = "wasm32")]
pub fn watch_mark_activation(
    container_id: &str,
    on_open: dioxus::prelude::EventHandler<Vec<String>>,
) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    let Some(root) = container(container_id) else {
        return;
    };
    if root.get_attribute("data-kb-marks").is_some() {
        return;
    }
    let _ = root.set_attribute("data-kb-marks", "1");
    let owners_of = |evt: &web_sys::Event| -> Option<Vec<String>> {
        let target = evt.target()?.dyn_into::<web_sys::Element>().ok()?;
        let mark = target.closest("mark[data-comment]").ok()??;
        Some(
            mark.get_attribute("data-comment")?
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        )
    };
    let click = Closure::wrap(Box::new(move |evt: web_sys::Event| {
        if let Some(ids) = owners_of(&evt) {
            on_open.call(ids);
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    let key = Closure::wrap(Box::new(move |evt: web_sys::KeyboardEvent| {
        let k = evt.key();
        if k != "Enter" && k != " " {
            return;
        }
        let as_event: &web_sys::Event = evt.as_ref();
        if let Some(ids) = owners_of(as_event) {
            evt.prevent_default();
            on_open.call(ids);
        }
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
    let _ = root.add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
    let _ = root.add_event_listener_with_callback("keydown", key.as_ref().unchecked_ref());
    // Lives for the container's lifetime; the page unmount drops the DOM.
    click.forget();
    key.forget();
}

/// Bring the first mark owned by `comment_id` into view and focus it.
#[cfg(target_arch = "wasm32")]
pub fn focus_mark(container_id: &str, comment_id: &str) {
    use wasm_bindgen::JsCast;
    let Some(root) = container(container_id) else {
        return;
    };
    let Ok(marks) = root.query_selector_all("mark[data-comment]") else {
        return;
    };
    for i in 0..marks.length() {
        let Some(el) = marks
            .get(i)
            .and_then(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
        else {
            continue;
        };
        let owns = el
            .get_attribute("data-comment")
            .unwrap_or_default()
            .split(',')
            .any(|id| id == comment_id);
        if owns {
            el.scroll_into_view_with_bool(true);
            let _ = el.focus();
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// desktop: the same logic as a script, answers back over the eval channel
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
use super::dom::{in_runtime, js_string};

#[cfg(not(target_arch = "wasm32"))]
pub async fn text_of(container_id: &str) -> Option<String> {
    if !in_runtime() {
        return None;
    }
    let mut eval = dioxus::document::eval(&format!(
        "const el = document.getElementById({}); dioxus.send(el ? el.textContent : null);",
        js_string(container_id)
    ));
    eval.recv::<Option<String>>().await.ok().flatten()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_highlights(container_id: &str) {
    if !in_runtime() {
        return;
    }
    dioxus::document::eval(&format!(
        "const el = document.getElementById({}); if (!el) return; \
         for (const m of Array.from(el.querySelectorAll('mark[data-comment]'))) {{ \
            while (m.firstChild) m.parentNode.insertBefore(m.firstChild, m); m.remove(); }} \
         el.normalize();",
        js_string(container_id)
    ));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn apply_highlights(container_id: &str, specs: &[MarkSpec]) {
    if !in_runtime() {
        return;
    }
    let specs_json = serde_json::to_string(
        &specs
            .iter()
            .map(|s| {
                serde_json::json!({
                    "start": s.start, "end": s.end, "ids": s.ids.join(","),
                    "class": mark_class(s.ids.len()), "label": aria_label(s.ids.len()),
                })
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_string());
    dioxus::document::eval(&format!(
        "const el = document.getElementById({id}); if (!el) return; \
         const specs = {specs}; \
         const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT); \
         const nodes = []; let pos = 0; let n; \
         while ((n = walker.nextNode())) {{ const len = Array.from(n.data).length; nodes.push([n, pos, pos + len]); pos += len; }} \
         const u16 = (s, chars) => Array.from(s).slice(0, chars).join('').length; \
         for (const [text, ns, ne] of nodes) {{ \
            let cur = text; let curStart = ns; \
            for (const sp of specs) {{ \
               if (!(sp.start < ne && sp.end > ns)) continue; \
               const ps = Math.max(sp.start, ns), pe = Math.min(sp.end, ne); \
               if (ps > curStart) {{ cur = cur.splitText(u16(cur.data, ps - curStart)); curStart = ps; }} \
               const after = cur.splitText(u16(cur.data, pe - ps)); \
               const mark = document.createElement('mark'); \
               mark.setAttribute('data-comment', sp.ids); mark.setAttribute('class', sp.class); \
               mark.setAttribute('tabindex', '0'); mark.setAttribute('role', 'button'); \
               mark.setAttribute('aria-label', sp.label); \
               cur.parentNode.insertBefore(mark, cur); mark.appendChild(cur); \
               cur = after; curStart = pe; \
            }} \
         }}",
        id = js_string(container_id),
        specs = specs_json,
    ));
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn mark_positions(container_id: &str) -> Vec<MarkPosition> {
    if !in_runtime() {
        return Vec::new();
    }
    let mut eval = dioxus::document::eval(&format!(
        "const el = document.getElementById({}); if (!el) {{ dioxus.send([]); return; }} \
         const top = el.getBoundingClientRect().top; const seen = new Set(); const out = []; \
         for (const m of el.querySelectorAll('mark[data-comment]')) {{ \
            const ids = m.getAttribute('data-comment'); if (seen.has(ids)) continue; seen.add(ids); \
            out.push({{ ids: ids.split(',').filter(Boolean), top: m.getBoundingClientRect().top - top }}); }} \
         dioxus.send(out);",
        js_string(container_id)
    ));
    #[derive(serde::Deserialize)]
    struct Row {
        ids: Vec<String>,
        top: f64,
    }
    eval.recv::<Vec<Row>>()
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|r| MarkPosition {
                    ids: r.ids,
                    top: r.top,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn selection_in(container_id: &str) -> Option<std::ops::Range<usize>> {
    if !in_runtime() {
        return None;
    }
    let mut eval = dioxus::document::eval(&format!(
        "const el = document.getElementById({}); const sel = window.getSelection(); \
         if (!el || !sel || sel.rangeCount === 0 || sel.isCollapsed) {{ dioxus.send(null); return; }} \
         const r = sel.getRangeAt(0); if (!el.contains(r.commonAncestorContainer)) {{ dioxus.send(null); return; }} \
         const before = document.createRange(); before.selectNodeContents(el); before.setEnd(r.startContainer, r.startOffset); \
         const start = Array.from(before.toString()).length; const len = Array.from(r.toString()).length; \
         dioxus.send(len > 0 ? [start, start + len] : null);",
        js_string(container_id)
    ));
    eval.recv::<Option<(usize, usize)>>()
        .await
        .ok()
        .flatten()
        .map(|(s, e)| s..e)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_selection() {
    if !in_runtime() {
        return;
    }
    dioxus::document::eval("const s = window.getSelection(); if (s) s.removeAllRanges();");
}

#[cfg(not(target_arch = "wasm32"))]
pub fn watch_mark_activation(
    container_id: &str,
    on_open: dioxus::prelude::EventHandler<Vec<String>>,
) {
    if !in_runtime() {
        return;
    }
    let mut eval = dioxus::document::eval(&format!(
        "const el = document.getElementById({}); if (!el || el.dataset.kbMarks) return 'installed'; \
         el.dataset.kbMarks = '1'; \
         const owners = (e) => {{ const m = e.target && e.target.closest && e.target.closest('mark[data-comment]'); \
            return m ? m.getAttribute('data-comment').split(',').filter(Boolean) : null; }}; \
         el.addEventListener('click', (e) => {{ const ids = owners(e); if (ids) dioxus.send(ids); }}); \
         el.addEventListener('keydown', (e) => {{ if (e.key !== 'Enter' && e.key !== ' ') return; \
            const ids = owners(e); if (ids) {{ e.preventDefault(); dioxus.send(ids); }} }}); \
         return 'installed';",
        js_string(container_id)
    ));
    dioxus::prelude::spawn(async move {
        loop {
            match eval.recv::<Vec<String>>().await {
                Ok(ids) => on_open.call(ids),
                Err(e) => {
                    tracing::error!("stopped listening for inline comment marks: {e}");
                    return;
                }
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn focus_mark(container_id: &str, comment_id: &str) {
    if !in_runtime() {
        return;
    }
    dioxus::document::eval(&format!(
        "const el = document.getElementById({}); if (!el) return; const id = {}; \
         for (const m of el.querySelectorAll('mark[data-comment]')) {{ \
            if (m.getAttribute('data-comment').split(',').includes(id)) {{ m.scrollIntoView(true); m.focus(); return; }} }}",
        js_string(container_id),
        js_string(comment_id)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_never_relies_on_colour_alone_and_steps_with_depth() {
        for depth in [1, 2, 3] {
            let cls = mark_class(depth);
            assert!(cls.contains("underline"), "depth {depth}: {cls}");
            assert!(
                cls.contains("focus:ring-2"),
                "depth {depth}: a visible focus ring"
            );
            assert!(cls.contains("kb-mark"));
        }
        assert_ne!(mark_class(1), mark_class(2));
        assert_ne!(mark_class(2), mark_class(3));
        assert_eq!(
            mark_class(9),
            mark_class(3),
            "deeper stacks share the top step"
        );
    }

    #[test]
    fn a_cut_is_measured_in_utf16_units() {
        assert_eq!(utf16_len("abc", 2), 2);
        assert_eq!(
            utf16_len("a\u{1F600}b", 2),
            3,
            "an astral char is two units"
        );
        assert_eq!(utf16_len("é", 1), 1);
    }

    #[test]
    fn the_label_counts() {
        assert_eq!(aria_label(1), "Inline comment");
        assert_eq!(aria_label(3), "3 inline comments");
    }
}

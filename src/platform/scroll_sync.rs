//! Keep two scroll boxes at the same relative position (MAPPS-600).
//!
//! The KB editor's split view is a `<textarea>` of Markdown beside a rendered
//! preview, and until this they were two unrelated scroll boxes: reading the
//! output of the paragraph you were editing meant scrolling both and guessing.
//!
//! ## Proportional, not line-mapped
//!
//! "You are 40% down the source" becomes "put the preview 40% down". It is NOT
//! "this line renders as that element", which is what the JetBrains editor this
//! was measured against does. That needs the renderer to emit source positions
//! per block; `pulldown-cmark` can (`OffsetIter` yields a byte range per event),
//! but carrying it through means `data-*` attributes on every block, a wider
//! ammonia allowlist to keep them, and the published article carrying editor
//! scaffolding.
//!
//! Where proportional is visibly wrong is where source density and render
//! density diverge: a long table is many source lines and renders short, so the
//! preview runs ahead of the source through it. That is the known cost, and it
//! is still far better than two panes in unrelated places.
//!
//! ## Sync to cursor (PMS-949)
//!
//! An image is that divergence at its worst - one source line, several hundred
//! rendered ones - and it was reported as "difficult to match after scrolling a
//! bit". [`scroll_to_block`] is the way back: it puts the preview on the block
//! the caret is in, exactly, on demand.
//!
//! It costs nothing the paragraph above rejected. The caret is exact already
//! (`selectionStart`), and a top-level block renders as one child element, so
//! the Nth block is the Nth child and no `data-*` attribute has to survive the
//! renderer, the sanitizer and the published article to say which is which.
//! `crate::utils::markdown::top_level_block_ranges` supplies the numbering.
//!
//! ## Why the echo guard earns its place
//!
//! Setting the other pane's `scrollTop` fires ITS scroll event, which answers by
//! scrolling this one back. That does NOT spin forever, because a write that
//! lands on the value an element already holds fires no event, so the exchange
//! stops as soon as the round trip returns the same pixel.
//!
//! It does not always return the same pixel. For extents S and P, the round trip
//! is `round(round(x/S*P)/P*S)`, and that differs from `x` for roughly half of
//! all positions whenever S > P: with the panes measured in a browser here,
//! 4049px against 2080px, it misses for 1969 of 4050 positions. The visible
//! effect is not a hang, it is that the pane you are scrolling jumps a pixel
//! under your hand, and only on the pane with the larger extent.
//!
//! Measured with the guard removed: driving the 4049px pane produced a second
//! scroll event on it, and driving the 2080px pane produced none. The guard is
//! a shared flag: a scroll caused by us is swallowed instead of answered, so the
//! pane the author is dragging keeps the position they chose.

#[cfg(all(feature = "app", target_arch = "wasm32"))]
use std::cell::Cell;
#[cfg(all(feature = "app", target_arch = "wasm32"))]
use std::rc::Rc;

/// Mark this pair of ids as synced. Idempotent: a second call for the same
/// pair does nothing, so it is safe to run from an effect that re-runs.
#[cfg(all(feature = "app", target_arch = "wasm32"))]
pub fn link(source_id: &str, preview_id: &str) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let (Some(source), Some(preview)) = (
        document.get_element_by_id(source_id),
        document.get_element_by_id(preview_id),
    ) else {
        return;
    };
    // Installing twice would double every scroll event and re-create the guard,
    // which defeats it. The marker rides on the element so it survives a
    // re-render of the component that asked for the link.
    if source.has_attribute(INSTALLED_ATTR) {
        return;
    }
    let _ = source.set_attribute(INSTALLED_ATTR, "1");
    let _ = preview.set_attribute(INSTALLED_ATTR, "1");

    // One flag for the pair: whichever pane is being driven sets it, and the
    // resulting scroll event on the other is swallowed rather than answered.
    let echo = Rc::new(Cell::new(false));

    for (from, to) in [(&source, &preview), (&preview, &source)] {
        // Two handles: one moves into the closure to read the position, the
        // other stays here to attach the listener to.
        let listener_target = from.clone();
        let from = from.clone();
        let to = to.clone();
        let echo = echo.clone();
        let cb = Closure::wrap(Box::new(move |_evt: web_sys::Event| {
            // PMS-949: a jump we made ourselves, from "sync to cursor". It is
            // not a position the author chose to be proportional to, so it is
            // swallowed rather than answered - otherwise landing the preview on
            // the caret's block would drag the source pane away from the caret.
            if take_programmatic() {
                return;
            }
            if echo.get() {
                echo.set(false);
                return;
            }
            let Some(frac) = scrolled_fraction(&from) else {
                // Nothing to read a position from: a pane shorter than its own
                // box has no scroll position, and forcing the other to the top
                // because of it would be a jump the author did not ask for.
                return;
            };
            let Some(target) = scrollable_extent(&to) else {
                return;
            };
            echo.set(true);
            to.set_scroll_top((f64::from(target) * frac).round() as i32);
        }) as Box<dyn FnMut(web_sys::Event)>);
        if listener_target
            .add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref())
            .is_err()
        {
            tracing::error!("could not attach the split-view scroll listener");
            return;
        }
        // Lives as long as the elements; the page unmount drops the DOM.
        cb.forget();
    }
}

/// How far down its own scrollable extent this element is, `0.0` to `1.0`.
/// `None` when it does not scroll at all.
#[cfg(all(feature = "app", target_arch = "wasm32"))]
fn scrolled_fraction(el: &web_sys::Element) -> Option<f64> {
    let extent = scrollable_extent(el)?;
    Some((f64::from(el.scroll_top()) / f64::from(extent)).clamp(0.0, 1.0))
}

/// Pixels this element can actually scroll through, or `None` when that is
/// zero: dividing by it is how a short pane would drag the other to the top.
#[cfg(all(feature = "app", target_arch = "wasm32"))]
fn scrollable_extent(el: &web_sys::Element) -> Option<i32> {
    let extent = el.scroll_height() - el.client_height();
    (extent > 0).then_some(extent)
}

// Raised immediately before a scroll this module performs on purpose, and taken
// by the first scroll event that follows. See the caller in `link`.
//
// Global rather than per-pair, because it guards a single user action that no
// second editor on the page can be performing at the same instant, and because
// the pair's own `echo` flag is owned by the closures `link` builds. It is only
// ever raised when the write actually changes the value, so it cannot be left
// set by a scroll that fired no event.
#[cfg(all(feature = "app", target_arch = "wasm32"))]
thread_local! {
    static PROGRAMMATIC: Cell<bool> = const { Cell::new(false) };
}

#[cfg(all(feature = "app", target_arch = "wasm32"))]
fn take_programmatic() -> bool {
    PROGRAMMATIC.with(|f| f.replace(false))
}

/// Put the preview's scroll box on its `index`th top-level block.
///
/// PMS-949: the answer to a tall image pulling the panes apart. The proportional
/// link is right about "40% down" and wrong about which paragraph that is, so
/// this maps the caret exactly instead: `crate::utils::markdown` says which
/// top-level block the caret is in, and the Nth block renders as the Nth child
/// of the rendered document.
///
/// `expected` is the number of blocks the source has. When the DOM holds a
/// different number of children the correspondence has broken (ammonia can drop
/// a raw HTML block), and this does nothing rather than scrolling somewhere
/// confidently wrong. Returns whether it moved anything.
#[cfg(all(feature = "app", target_arch = "wasm32"))]
pub fn scroll_to_block(preview_id: &str, index: usize, expected: usize) -> bool {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return false;
    };
    let Some(box_el) = document.get_element_by_id(preview_id) else {
        return false;
    };
    // The scroll box holds one child, the rendered document, and its children
    // are the blocks. `Markdown` owns that element and its id; this only counts
    // through it.
    let Some(rendered) = box_el.first_element_child() else {
        return false;
    };
    if rendered.child_element_count() as usize != expected {
        return false;
    }
    // Walked rather than indexed through `children()`: that returns an
    // `HtmlCollection`, which is a web-sys feature this crate does not enable,
    // and a sibling walk needs no new binding.
    let mut target = rendered.first_element_child();
    for _ in 0..index {
        target = target.and_then(|el| el.next_element_sibling());
    }
    let Some(target) = target else {
        return false;
    };
    // Relative to the box's own scrolled content, which is what `scrollTop`
    // counts: `offsetTop` would be relative to the nearest positioned ancestor
    // and neither element positions itself.
    let top = target.get_bounding_client_rect().top() - box_el.get_bounding_client_rect().top()
        + f64::from(box_el.scroll_top());
    let next = top.round().max(0.0) as i32;
    if next == box_el.scroll_top() {
        return false;
    }
    PROGRAMMATIC.with(|f| f.set(true));
    box_el.set_scroll_top(next);
    true
}

/// MAPPS-699: the same jump, performed in the webview.
///
/// The walk, the count check and the offset arithmetic are the browser's,
/// restated in JavaScript, because none of it needs a value back in Rust. The
/// caret that produced `index` does: the caller reads it through
/// `platform::dom::textarea_selection`, which answers from the webview only
/// while [`link`] has its reporter installed - and it installs it for exactly
/// the pane this button syncs.
///
/// Returns whether the script was dispatched, not whether the preview moved.
/// That answer is only known in the webview and would have to come back
/// asynchronously; no caller reads the value, and the alternative is reporting
/// a fixed `false` that a stub is indistinguishable from.
#[cfg(not(all(feature = "app", target_arch = "wasm32")))]
pub fn scroll_to_block(preview_id: &str, index: usize, expected: usize) -> bool {
    if !crate::platform::dom::in_runtime() {
        return false;
    }
    crate::platform::dom::eval(&format!(
        "const boxEl = document.getElementById({}); \
         if (!boxEl) return; \
         const rendered = boxEl.firstElementChild; \
         if (!rendered) return; \
         if (rendered.childElementCount !== {expected}) return; \
         let target = rendered.firstElementChild; \
         for (let i = 0; i < {index}; i++) target = target && target.nextElementSibling; \
         if (!target) return; \
         const top = Math.max(Math.round(target.getBoundingClientRect().top \
            - boxEl.getBoundingClientRect().top + boxEl.scrollTop), 0); \
         if (top === boxEl.scrollTop) return; \
         window.{PROGRAMMATIC_FLAG} = true; \
         boxEl.scrollTop = top;",
        crate::platform::dom::js_string(preview_id)
    ));
    true
}

/// Where the desktop keeps the flag the browser keeps in `PROGRAMMATIC`. On
/// `window` because each `eval` is a separate script with its own scope, so the
/// jump and the scroll listener that has to swallow it have no other way to
/// name the same variable.
#[cfg(not(all(feature = "app", target_arch = "wasm32")))]
const PROGRAMMATIC_FLAG: &str = "__mokoshScrollProgrammatic";

#[cfg(all(feature = "app", target_arch = "wasm32"))]
const INSTALLED_ATTR: &str = "data-scroll-synced";

/// MAPPS-699: the same link, wired inside the webview.
///
/// One injected script and no channel, because nothing has to come back: both
/// listeners, the proportional mapping and the echo guard all read and write
/// the same document the script runs in. Every rule above is restated here in
/// JavaScript - install once, a pane that cannot scroll does not drag the other,
/// and a scroll this module caused is swallowed rather than answered - so the
/// two hosts differ in language and not in behaviour.
///
/// It also installs the caret reporter for the source pane. "Sync to cursor"
/// ([`scroll_to_block`]) needs `selectionStart`, and a desktop read is
/// asynchronous while that button is a click handler that has to answer at
/// once; the reporter pushes the value instead. It belongs here because this is
/// the call that runs exactly when the split view exists, which is the only
/// place that button is rendered.
#[cfg(not(all(feature = "app", target_arch = "wasm32")))]
pub fn link(source_id: &str, preview_id: &str) {
    crate::platform::dom::eval(&format!(
        "const source = document.getElementById({source}); \
         const preview = document.getElementById({preview}); \
         if (!source || !preview) return; \
         if (source.dataset.scrollSynced) return; \
         source.dataset.scrollSynced = '1'; \
         preview.dataset.scrollSynced = '1'; \
         const state = {{ echo: false }}; \
         const extent = (el) => {{ const e = el.scrollHeight - el.clientHeight; \
            return e > 0 ? e : null; }}; \
         const wire = (from, to) => from.addEventListener('scroll', () => {{ \
            if (window.{PROGRAMMATIC_FLAG}) {{ window.{PROGRAMMATIC_FLAG} = false; return; }} \
            if (state.echo) {{ state.echo = false; return; }} \
            const span = extent(from); if (span === null) return; \
            const reach = extent(to); if (reach === null) return; \
            state.echo = true; \
            to.scrollTop = Math.round(reach * Math.min(Math.max(from.scrollTop / span, 0), 1)); \
         }}); \
         wire(source, preview); \
         wire(preview, source);",
        source = crate::platform::dom::js_string(source_id),
        preview = crate::platform::dom::js_string(preview_id),
    ));
    crate::platform::dom::watch_textarea_selection(source_id);
}

#[cfg(test)]
mod tests {
    const SRC: &str = include_str!("scroll_sync.rs");

    fn code_only() -> String {
        let end = SRC.find("mod tests").expect("tests are in this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The echo guard keeps the pane the author is dragging at the position
    /// they chose. Setting the other pane's scrollTop fires its scroll event,
    /// and without the flag that event scrolls this one back to whatever the
    /// round trip produced, which is a different pixel about half the time.
    /// See the module header for the measurement.
    #[test]
    fn a_driven_scroll_is_swallowed_rather_than_answered() {
        let code = code_only();
        assert!(
            code.contains("if echo.get() { echo.set(false); return; }"),
            "{code:?}"
        );
        assert!(
            code.contains("echo.set(true); to.set_scroll_top("),
            "the flag is raised immediately before driving the other pane"
        );
    }

    /// PMS-949: a jump this module makes is not a position the author chose, so
    /// the proportional link must not answer it. Without this, landing the
    /// preview on the caret's block would immediately drag the source pane away
    /// from the caret, which is the one thing the button exists to avoid.
    #[test]
    fn a_sync_to_cursor_jump_is_not_answered_proportionally() {
        let code = code_only();
        let taken = code
            .find("if take_programmatic() { return; }")
            .expect("the scroll listener takes the programmatic flag");
        let echo = code
            .find("if echo.get() { echo.set(false); return; }")
            .expect("and still has the echo guard");
        assert!(taken < echo, "before the echo guard, not instead of it");
        assert!(
            code.contains("PROGRAMMATIC.with(|f| f.set(true)); box_el.set_scroll_top(next);"),
            "and the flag is raised immediately before the jump that fires the event"
        );
        assert!(
            code.contains("if next == box_el.scroll_top() { return false; }"),
            "a write that changes nothing fires no event, so it must not raise the \
             flag either - it would be taken by the author's next real scroll"
        );
    }

    /// The block map is an assumption about the DOM, so it is checked against
    /// the DOM. Ammonia can drop a raw HTML block, and every index after it
    /// then names the wrong element; scrolling somewhere confidently wrong is
    /// worse than not scrolling.
    #[test]
    fn the_block_jump_gives_up_when_the_counts_disagree() {
        let code = code_only();
        assert!(
            code.contains(
                "if rendered.child_element_count() as usize != expected { return false; }"
            ),
            "the rendered child count is compared against the source's block count"
        );
    }

    /// The arithmetic the guard exists for, checked here rather than asserted
    /// in a comment: a proportional round trip does not return the pixel it
    /// started from when the driving pane has the larger extent.
    ///
    /// This is why the echo shows up on one pane and not the other, which is
    /// exactly the shape of bug that gets reported as "it feels fine most of
    /// the time".
    #[test]
    fn the_round_trip_does_not_land_where_it_started() {
        fn round_trip(x: i64, from: i64, to: i64) -> i64 {
            let y = ((x as f64 / from as f64) * to as f64).round() as i64;
            ((y as f64 / to as f64) * from as f64).round() as i64
        }
        // The extents measured in a browser for a table-heavy article.
        let (big, small) = (4049i64, 2080i64);
        let misses = (0..=big)
            .filter(|x| round_trip(*x, big, small) != *x)
            .count();
        assert!(
            misses > big as usize / 3,
            "driving the larger pane misses often: {misses} of {}",
            big + 1
        );
        let misses = (0..=small)
            .filter(|x| round_trip(*x, small, big) != *x)
            .count();
        assert_eq!(
            misses, 0,
            "and driving the smaller pane does not, which is why this only bites one way"
        );
    }

    /// One flag for the PAIR, not one per direction: an echo raised by driving
    /// the preview has to be recognised by the source's own listener.
    #[test]
    fn the_guard_is_shared_between_both_directions() {
        let code = code_only();
        assert_eq!(
            code.matches("let echo = Rc::new(Cell::new(false));")
                .count(),
            1,
            "exactly one flag, cloned into both listeners"
        );
        assert!(code.contains("let echo = echo.clone();"));
    }

    /// Attaching twice doubles every scroll event and re-creates the guard,
    /// which defeats it. The marker rides on the element so it survives a
    /// re-render of whatever asked for the link.
    #[test]
    fn linking_twice_is_a_no_op() {
        let code = code_only();
        assert!(code.contains("if source.has_attribute(INSTALLED_ATTR) { return; }"));
    }

    /// A pane too short to scroll has no position to read, and answering it
    /// with 0.0 would yank the other pane to the top for no reason.
    #[test]
    fn a_pane_that_cannot_scroll_does_not_drag_the_other() {
        let code = code_only();
        assert!(
            code.contains("(extent > 0).then_some(extent)"),
            "zero extent is None, not a division"
        );
        assert!(
            code.contains("let Some(frac) = scrolled_fraction(&from) else {"),
            "and a None short-circuits instead of scrolling"
        );
    }

    /// MAPPS-699: the desktop half, pinned in source the way
    /// `platform::dom::desktop_wiring_tests` pins the rest of the channel.
    ///
    /// It needs a webview to evaluate JavaScript in, so no host test can drive
    /// it. What can regress is the wiring, and both of these went missing once
    /// already as stubs that answered "nothing to do" and said nothing about it.
    #[test]
    fn the_desktop_wires_both_panes_inside_the_webview() {
        let code = code_only();
        assert_eq!(
            code.matches("pub fn link(source_id: &str, preview_id: &str)")
                .count(),
            2,
            "one implementation per target, and neither is an empty body"
        );
        assert!(
            code.contains("wire(source, preview);") && code.contains("wire(preview, source);"),
            "the injected script attaches a listener in each direction"
        );
        assert!(
            code.contains("source.dataset.scrollSynced = '1';"),
            "and marks the pair, so a re-running effect does not double every \
             scroll event"
        );
        assert!(
            code.contains("const span = extent(from); if (span === null) return;"),
            "a pane that cannot scroll still does not drag the other"
        );
    }

    /// Both guards survive the translation, or the desktop gets exactly the
    /// bugs the browser implementation was written to avoid.
    #[test]
    fn the_desktop_script_keeps_both_guards() {
        let code = code_only();
        let programmatic = code
            .find("if (window.{PROGRAMMATIC_FLAG})")
            .expect("the script takes the programmatic flag");
        let echo = code
            .find("if (state.echo)")
            .expect("and still has the echo guard");
        assert!(
            programmatic < echo,
            "before the echo guard, not instead of it"
        );
        assert!(
            code.contains("state.echo = true;"),
            "the echo flag is raised immediately before driving the other pane"
        );
        assert!(
            code.contains("window.{PROGRAMMATIC_FLAG} = true;"),
            "and the programmatic flag immediately before the sync-to-cursor jump"
        );
        assert!(
            code.contains("if (top === boxEl.scrollTop) return;"),
            "a write that changes nothing fires no event, so it must not raise \
             the flag either"
        );
    }

    /// MAPPS-699: the desktop jump reads the caret, or it lands on block zero.
    ///
    /// `platform::dom::textarea_selection` answers from the webview only for an
    /// element something is watching, and this is what puts the source pane on
    /// that list. Without it the index the caller computes is always the one
    /// for offset zero and the button scrolls the preview to the top.
    #[test]
    fn the_desktop_jump_has_a_caret_to_work_from() {
        let code = code_only();
        assert_eq!(
            code.matches("pub fn scroll_to_block(preview_id: &str, index: usize, expected: usize)")
                .count(),
            2,
            "one implementation per target, and neither ignores its arguments"
        );
        assert!(
            code.contains("crate::platform::dom::watch_textarea_selection(source_id);"),
            "linking the panes installs the caret reporter for the source pane"
        );
        assert!(
            code.contains("if (rendered.childElementCount !== {expected}) return;"),
            "and the desktop checks the block counts agree, exactly as the \
             browser does"
        );
    }
}

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

#[cfg(all(feature = "web", target_arch = "wasm32"))]
use std::cell::Cell;
#[cfg(all(feature = "web", target_arch = "wasm32"))]
use std::rc::Rc;

/// Mark this pair of ids as synced. Idempotent: a second call for the same
/// pair does nothing, so it is safe to run from an effect that re-runs.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
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
#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn scrolled_fraction(el: &web_sys::Element) -> Option<f64> {
    let extent = scrollable_extent(el)?;
    Some((f64::from(el.scroll_top()) / f64::from(extent)).clamp(0.0, 1.0))
}

/// Pixels this element can actually scroll through, or `None` when that is
/// zero: dividing by it is how a short pane would drag the other to the top.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn scrollable_extent(el: &web_sys::Element) -> Option<i32> {
    let extent = el.scroll_height() - el.client_height();
    (extent > 0).then_some(extent)
}

#[cfg(all(feature = "web", target_arch = "wasm32"))]
const INSTALLED_ATTR: &str = "data-scroll-synced";

/// MAPPS-504 / MAPPS-511: the desktop build has no in-process DOM, so the two
/// panes stay independent there. The editor is usable either way; this is
/// polish, not function.
#[cfg(not(all(feature = "web", target_arch = "wasm32")))]
pub fn link(_source_id: &str, _preview_id: &str) {}

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
}

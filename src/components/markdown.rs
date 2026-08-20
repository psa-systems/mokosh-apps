//! Render Markdown text for display (PMS-309).
//!
//! Thin wrapper over [`crate::utils::markdown::render_markdown`] (which
//! sanitizes with ammonia) plus Tailwind `prose` styling, so detail-page
//! descriptions render the same way KB article bodies do. Use for any
//! read-only free-text field whose authors may write Markdown.
//!
//! With `interactive` set, task-list checkboxes become clickable: each
//! rendered checkbox carries a `data-ti="<index>"` attribute, and a single
//! delegated click listener on the container reports the toggled index to
//! `on_toggle` so the host can flip the source marker and persist it
//! (PMS-348).

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct MarkdownProps {
    /// Raw Markdown source.
    content: String,
    /// Extra classes appended to the `prose` wrapper.
    #[props(default)]
    class: String,
    /// When true, task-list checkboxes are enabled and clicking one calls
    /// `on_toggle` with its 0-based document index (PMS-348).
    #[props(default = false)]
    interactive: bool,
    /// Fires with the toggled task index when `interactive`. No-op otherwise.
    #[props(default)]
    on_toggle: EventHandler<usize>,
}

#[component]
pub fn Markdown(props: MarkdownProps) -> Element {
    // Stable per-instance id so the delegated click listener can find this
    // container. Atomic counter is deterministic and wasm is single-threaded.
    let dom_id = use_hook(|| {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        format!("md-{}", NEXT.fetch_add(1, Ordering::Relaxed))
    });

    let html = if props.interactive {
        crate::utils::markdown::render_markdown_interactive(&props.content)
    } else {
        crate::utils::markdown::render_markdown(&props.content)
    };

    // One delegated click listener on the container: a checkbox carries
    // `data-ti="<index>"`, so reading it on click maps the click to a task
    // item. Attached once; it survives content re-renders because only the
    // container's innerHTML changes, not the element itself.
    #[cfg(feature = "web")]
    {
        let dom_id = dom_id.clone();
        let on_toggle = props.on_toggle;
        let interactive = props.interactive;
        use_effect(move || {
            if !interactive {
                return;
            }
            // MAPPS-504: the rendered markdown is raw HTML, so the
            // checkbox clicks are caught by one delegated listener rather
            // than per-element Dioxus handlers - and installing that
            // listener needs the DOM in-process. On the desktop the
            // checkboxes render but do not toggle. Tracked in MAPPS-511.
            #[cfg(target_arch = "wasm32")]
            install_toggle_listener(dom_id.clone(), on_toggle);
            #[cfg(not(target_arch = "wasm32"))]
            let _ = (&dom_id, on_toggle);
        });
    }

    rsx! {
        div {
            id: "{dom_id}",
            class: "prose dark:prose-invert max-w-none {props.class}",
            dangerous_inner_html: html,
        }
    }
}

/// Install a delegated `click` listener on the rendered markdown
/// container so a task-list checkbox reports its index back to the
/// caller.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn install_toggle_listener(dom_id: String, on_toggle: EventHandler<usize>) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    let Some(container) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&dom_id))
    else {
        return;
    };
    let cb = Closure::wrap(Box::new(move |evt: web_sys::Event| {
        let Some(el) = evt
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        if let Some(idx) = el
            .get_attribute("data-ti")
            .and_then(|s| s.parse::<usize>().ok())
        {
            on_toggle.call(idx);
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    if container
        .add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())
        .is_err()
    {
        // The checkboxes would render and then do nothing when clicked.
        tracing::error!("could not attach the markdown task-list click listener");
        return;
    }
    // Lives for the container's lifetime; the page unmount drops the DOM.
    cb.forget();
}

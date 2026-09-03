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
//!
//! MAPPS-578: `@handle` resolves against the tenant's staff directory, which
//! this component fetches once and shares across every instance on the page.
//! A resolved mention renders as a chip naming the person; anything that does
//! not resolve stays the plain text the author wrote. The chip routes to the
//! team roster on click, but only for a viewer who can open it, so a reader
//! without access gets an informative chip rather than a link that 403s.

use dioxus::prelude::*;

use crate::utils::mentions::Mention;

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
    /// MAPPS-578: resolve `@handle` against the staff directory. Off by
    /// default so a surface that renders untrusted or historical text can opt
    /// out; every in-app caller wants it on.
    #[props(default = true)]
    mentions: bool,
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

    // MAPPS-578: one fetch per page, not one per Markdown instance. A ticket
    // page renders several of these, and the directory is the same for all of
    // them. A failure leaves it empty, which renders every `@` as the plain
    // text it already was, so a technician who cannot read `/auth/users` (it is
    // manager-gated) sees exactly what shipped before this.
    let directory = crate::hooks::use_mention_directory(props.mentions);
    let people: Vec<Mention> = crate::hooks::mention_people(&directory);

    // MAPPS-595: the origin an attachment path is joined onto. Empty in dev,
    // where the SPA and the API already share one; the API's own origin on a
    // split-origin deployment, where a bare `/api/v1/...` would be answered by
    // the SPA's own fallback with an HTML document and render as a broken
    // image. This component is the only place article markdown becomes HTML,
    // so it is the only place the join has to happen.
    let api_origin = crate::hooks::fetch::api::api_origin();
    let html = if props.interactive {
        crate::utils::markdown::render_markdown_interactive_with_mentions(
            &props.content,
            &people,
            &api_origin,
        )
    } else {
        crate::utils::markdown::render_markdown_with_mentions(&props.content, &people, &api_origin)
    };

    // One delegated click listener on the container, for the task-list
    // checkboxes. A checkbox carries `data-ti="<index>"`, so reading the
    // attribute on click maps it back to the item. Attached once; it survives
    // content re-renders because only the container's innerHTML changes, not
    // the element itself.
    //
    // MAPPS-603: a mention chip is NOT clickable, for anybody.
    //
    // It used to route to `/admin/team` for an Admin and do nothing for anyone
    // else. That page does not list team members: it invites people and manages
    // pending invitations, and nothing in this app shows a user. So the click
    // navigated an admin away from what they were reading to a page that said
    // nothing about the person they clicked, and gave everyone else a chip that
    // looked different for no reason they could act on.
    //
    // What a mention is worth is identity, and the chip already carries it:
    // MAPPS-578 puts the name and email in `title`, which every reader gets on
    // hover whatever their role. Removing the navigation costs nothing that was
    // worth having and takes three pieces of machinery with it: the container
    // class that existed to make two kinds of chip look different (MAPPS-585),
    // the router call from a raw DOM listener that killed the page when it
    // panicked (MAPPS-586), and the role read that made an article render
    // differently for two people reading the same words.
    #[cfg(feature = "app")]
    {
        let dom_id = dom_id.clone();
        let on_toggle = props.on_toggle;
        let interactive = props.interactive;
        use_effect(move || {
            if !interactive {
                return;
            }
            // MAPPS-504: the rendered markdown is raw HTML, so these clicks are
            // caught by one delegated listener rather than per-element Dioxus
            // handlers. The browser attaches that listener itself; the desktop
            // cannot attach one from Rust, so MAPPS-511 has the injected script
            // attach it and post each click back over the `eval` channel.
            #[cfg(target_arch = "wasm32")]
            install_click_listener(dom_id.clone(), interactive.then_some(on_toggle));
            #[cfg(not(target_arch = "wasm32"))]
            crate::platform::dom::watch_task_toggles(&dom_id, on_toggle);
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

/// Install one delegated `click` listener on the rendered markdown container.
///
/// A task-list checkbox is raw HTML injected with `dangerous_inner_html` and
/// cannot carry a Dioxus handler, so it reports its `data-ti` index back to the
/// caller through this listener instead.
///
/// MAPPS-586: the callback is an `EventHandler`, and that is load-bearing
/// rather than stylistic. This closure runs from a raw DOM listener, so no
/// dioxus scope is on the stack; anything that asks the runtime which scope it
/// is in panics inside `Runtime::current_scope_id`, which unwraps an empty
/// stack WHILE holding a shared borrow of it. Release builds are
/// `panic = "abort"`, so nothing unwinds, the borrow guard never drops, and
/// every render afterwards panics on `scope_stack.borrow_mut()`. One click
/// killed the page for good. `EventHandler::call` pushes its origin scope
/// first, which is what a bare boxed closure does not do.
///
/// MAPPS-603 removed the other caller, a mention chip that routed to the team
/// page. The rule outlives it: whatever this listener calls next has to carry
/// its own scope.
#[cfg(all(feature = "app", target_arch = "wasm32"))]
fn install_click_listener(dom_id: String, on_toggle: Option<EventHandler<usize>>) {
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
        if let Some(on_toggle) = on_toggle {
            if let Some(idx) = el
                .get_attribute("data-ti")
                .and_then(|s| s.parse::<usize>().ok())
            {
                on_toggle.call(idx);
                return;
            }
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    if container
        .add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())
        .is_err()
    {
        // The checkboxes would render and then do nothing when clicked.
        tracing::error!("could not attach the markdown click listener");
        return;
    }
    // Lives for the container's lifetime; the page unmount drops the DOM.
    cb.forget();
}

/// MAPPS-578: the two decisions the mention chip rests on, neither of which a
/// rendered snapshot shows.
///
/// Source scans, deliberately: the directory fetch and the click listener only
/// run in the browser with a real DOM, so no host test can drive them. What is
/// pinned is the decision, and the decision lives in the source. The module is
/// excluded from its own scan because every assertion quotes the pattern it
/// looks for, which would otherwise match itself and pass regardless.
#[cfg(test)]
mod mention_wiring_tests {
    const SRC: &str = include_str!("markdown.rs");

    /// The shipping code with runs of whitespace collapsed, so an assertion
    /// pins the decision rather than whatever line breaks rustfmt chose this
    /// week. Excludes this module: every assertion quotes the pattern it looks
    /// for, so a scan including its own source matches itself and passes
    /// regardless of what the component does.
    fn code_only() -> String {
        let end = SRC
            .find("mod mention_wiring_tests")
            .expect("this module is part of this file");
        SRC[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// MAPPS-586: everything the raw click listener calls is an
    /// `EventHandler`.
    ///
    /// The listener runs from a plain DOM event, so nothing has pushed a
    /// dioxus scope. Anything that asks the runtime which scope it is in
    /// unwraps an empty stack while holding a shared borrow of it, and because
    /// release builds are `panic = "abort"` that borrow guard never drops.
    /// Every later render then panics on `scope_stack.borrow_mut()`, so a
    /// single click left the page permanently unresponsive.
    ///
    /// MAPPS-603 removed the caller that did it (the mention chip's
    /// navigation). The rule outlives it: the checkbox branch is what is left,
    /// and whatever is added next has to carry its own scope too.
    #[test]
    fn the_raw_listener_only_calls_things_that_carry_their_own_scope() {
        let code = code_only();
        assert!(
            !code.contains("Box<dyn Fn()>"),
            "a bare closure called from a DOM listener has no dioxus scope, and \
             a panic inside one poisons the runtime for the life of the page"
        );
        assert!(
            code.contains("on_toggle: Option<EventHandler<usize>>"),
            "the checkbox callback is an EventHandler, which pushes its origin \
             scope before running"
        );
    }

    /// MAPPS-603: a mention chip is inert, for every reader.
    ///
    /// It used to route to `/admin/team` for an Admin and do nothing for anyone
    /// else. That page does not list team members: it invites people and
    /// manages pending invitations, and nothing in this app shows a user. So
    /// the click took an admin away from what they were reading to a page that
    /// said nothing about the person they clicked.
    ///
    /// Pinned because "a mention should be clickable" is an easy assumption to
    /// re-make, and acting on it means finding somewhere for it to go first.
    #[test]
    fn a_mention_chip_does_not_navigate() {
        let code = code_only();
        assert!(
            !code.contains("navigator.push"),
            "nothing here navigates: the chip's worth is the name and email it \
             already carries in `title`"
        );
        assert!(
            !code.contains("data-mention"),
            "and the listener does not look for a chip at all"
        );
        assert!(
            !code.contains("mentions-open"),
            "so there is one kind of chip, not two"
        );
        assert!(
            !code.contains("UserRole::Admin"),
            "and an article renders the same for two people reading the same words"
        );
    }

    /// MAPPS-595: the render call passes the API origin.
    ///
    /// `utils::markdown` cannot know it, and this component is the only place
    /// article markdown becomes HTML, so this call is the single point where
    /// the join can be dropped. Dropping it is invisible in dev, where the
    /// origin is empty and the same-origin path already resolves, and breaks
    /// every image on every split-origin deployment. Pinned here because the
    /// two-argument form still reads as complete.
    #[test]
    fn the_render_call_carries_the_api_origin() {
        let code = code_only();
        assert!(
            code.contains("let api_origin = crate::hooks::fetch::api::api_origin();"),
            "the component resolves the API origin"
        );
        assert_eq!(
            code.matches("&api_origin").count(),
            2,
            "and passes it to BOTH renderers, interactive and not"
        );
    }

    /// MAPPS-592: list density lives in the stylesheet, and nothing else in
    /// the crate would notice it going missing.
    ///
    /// The plugin's `li { margin: 0.5em 0 }` put 8px between every bullet on
    /// top of a 28px line box, so a six-item list of hostnames ran to 211px and
    /// read as six paragraphs. The three rules below are the whole fix; the
    /// `check-prose-layer.sh` guard is what keeps them outranking the plugin,
    /// and this is what keeps them present and in the right order relative to
    /// each other.
    #[test]
    fn a_list_is_denser_than_the_prose_around_it() {
        const CSS: &str = include_str!("../../input.css");
        for rule in [
            ".prose :where(li):not(:where([class~=\"not-prose\"] *))",
            ".prose :where(li > p):not(:where([class~=\"not-prose\"] *))",
            ".prose :where(li > ul, li > ol):not(:where([class~=\"not-prose\"] *))",
        ] {
            assert!(CSS.contains(rule), "the stylesheet must carry `{rule}`");
        }
        // A tight item closes right up; a loose one (its text wrapped in a `<p>`
        // because the author left a blank line) keeps more air than that. The
        // ordering is the decision, so it is what gets asserted.
        let tight = CSS
            .find(".prose :where(li):not")
            .expect("the tight-item rule");
        let loose = CSS
            .find(".prose :where(li > p):not")
            .expect("the loose-item rule");
        assert!(
            CSS[tight..loose].contains("margin-top: 0.125em;"),
            "a tight list item is the densest of the three"
        );
        assert!(
            CSS[loose..].contains("margin-top: 0.5em;"),
            "a loose list item keeps more room than a tight one, because \
             Markdown distinguishes the two on purpose"
        );
    }

    /// MAPPS-511: the checkboxes are wired on the desktop too.
    ///
    /// They used to render there and do nothing, because the listener that
    /// carries a click out of `dangerous_inner_html` could only be attached
    /// from Rust. Pinned because the failure is silent: the checkbox is drawn
    /// either way, and a discarded handler looks like a cfg tidy-up.
    #[test]
    fn the_desktop_branch_installs_a_listener_too() {
        let code = code_only();
        assert!(
            code.contains("crate::platform::dom::watch_task_toggles(&dom_id, on_toggle);"),
            "the desktop branch hands the handler to the platform listener"
        );
        assert!(
            !code.contains("let _ = (&dom_id, on_toggle);"),
            "and does not discard it"
        );
    }

    /// MAPPS-592: the component reads the directory through the shared hook.
    ///
    /// Three surfaces need the same list and each had fetched it for itself,
    /// which is how the KB editor's copy came to fall back to the handle for a
    /// nameless row while this one did not. The endpoint choice (PMS-921's
    /// `/auth/directory`, never the manager-gated `/auth/users`) and the
    /// degrade-to-empty rule are pinned in `hooks::mentions` now, where they
    /// have one definition.
    #[test]
    fn the_directory_comes_from_the_shared_hook() {
        let code = code_only();
        assert!(
            code.contains("crate::hooks::use_mention_directory(props.mentions)"),
            "the fetch is not this component's to own"
        );
        assert!(
            !code.contains("get_all_authed"),
            "and it must not grow its own copy back"
        );
    }
}

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
    let directory = use_mention_directory(props.mentions);
    let people: Vec<Mention> = directory
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_default();

    let html = if props.interactive {
        crate::utils::markdown::render_markdown_interactive_with_mentions(&props.content, &people)
    } else {
        crate::utils::markdown::render_markdown_with_mentions(&props.content, &people)
    };

    // MAPPS-578: whether a mention chip should route on click. Only for a
    // viewer who can actually open the destination: `/admin/team` is an
    // admin-only surface, and there is no per-user page to send anyone else
    // to, so for everyone else the chip stays informative and inert rather
    // than being a link that 403s.
    let mention_target = {
        let auth = crate::hooks::use_auth();
        let a = auth.read();
        (a.has_role(crate::modules::auth::UserRole::Admin)
            || a.has_role(crate::modules::auth::UserRole::SuperAdmin))
        .then_some(crate::Route::Team {})
    };

    // One delegated click listener on the container: a checkbox carries
    // `data-ti="<index>"` and a mention chip carries `data-mention="<id>"`, so
    // reading the attributes on click maps it to whichever it was. Attached
    // once; it survives content re-renders because only the container's
    // innerHTML changes, not the element itself.
    #[cfg(feature = "web")]
    {
        let dom_id = dom_id.clone();
        let on_toggle = props.on_toggle;
        let interactive = props.interactive;
        let navigator = use_navigator();
        let mention_route = mention_target.clone();
        use_effect(move || {
            if !interactive && mention_route.is_none() {
                return;
            }
            // MAPPS-504: the rendered markdown is raw HTML, so these clicks are
            // caught by one delegated listener rather than per-element Dioxus
            // handlers - and installing that listener needs the DOM in-process.
            // On the desktop the checkboxes render but do not toggle. Tracked
            // in MAPPS-511.
            #[cfg(target_arch = "wasm32")]
            install_click_listener(
                dom_id.clone(),
                interactive.then_some(on_toggle),
                // MAPPS-586: an `EventHandler`, not a bare closure.
                //
                // This runs from a raw DOM listener, so there is no dioxus
                // scope on the stack. `navigator.push` writes the router's
                // signals, which asks the runtime for the current scope, and
                // `Runtime::current_scope_id` unwraps an empty stack. That
                // panic fires while it still holds a shared borrow of the
                // scope stack, and the release profile is `panic = "abort"`,
                // so nothing unwinds and the borrow guard never drops. From
                // that moment every render in the page panics on
                // `scope_stack.borrow_mut()` and the whole app is dead. The
                // reported symptom was a page that stopped responding.
                //
                // `EventHandler::call` is what makes this safe: it takes a
                // `RuntimeGuard` and pushes its origin scope before running
                // the body, so the navigation happens inside the scope it was
                // created in. The sibling checkbox branch has always gone
                // through one, which is why only mentions killed the page.
                mention_route.clone().map(|route| {
                    EventHandler::new(move |()| {
                        // The router reports an external-navigation failure,
                        // which cannot happen for an in-app route.
                        let _ = navigator.push(route.clone());
                    })
                }),
            );
            #[cfg(not(target_arch = "wasm32"))]
            let _ = (&dom_id, on_toggle, &mention_route, navigator);
        });
    }
    #[cfg(not(feature = "web"))]
    let _ = &mention_target;

    // MAPPS-585: say which chips are actually clickable.
    //
    // The rendered HTML is the same for every reader, because the markup is
    // built before anyone knows who is looking. Only the listener differs, so
    // until now an inert chip and a live one were pixel-identical: no pointer,
    // no hover, and a title naming the person rather than a destination. The
    // reporter asked what a mention was for, having clicked one that was
    // inert. The class goes on the container, which is the one element that
    // does know whether routing was wired.
    let mention_class = if mention_target.is_some() {
        " mentions-open"
    } else {
        ""
    };

    rsx! {
        div {
            id: "{dom_id}",
            class: "prose dark:prose-invert max-w-none{mention_class} {props.class}",
            dangerous_inner_html: html,
        }
    }
}

/// The tenant's staff directory, for resolving `@handle` (MAPPS-578).
///
/// `GET /auth/directory` (PMS-921), not `/auth/users`. The latter is
/// `RequireManager`, so resolving against it meant a Technician got a 403 and
/// saw every mention as plain text: a KB article is written for technicians and
/// its mentions assign ownership, so the reader who most needed to know who was
/// named was the one who could not see it. The directory is `RequireAuth` and
/// returns id, name and handle only.
///
/// A failure is still not an error state: it yields an empty directory and
/// every `@` renders as the plain text it already was. Nothing about a mention
/// is worth blocking an article over.
fn use_mention_directory(enabled: bool) -> Resource<Option<Vec<Mention>>> {
    use_resource(move || async move {
        if !enabled {
            return None;
        }
        #[cfg(feature = "web")]
        {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            #[derive(serde::Deserialize)]
            struct DirectoryEntry {
                id: uuid::Uuid,
                #[serde(default)]
                name: String,
                #[serde(default)]
                handle: String,
            }
            let rows =
                crate::hooks::fetch::api::get_all_authed::<DirectoryEntry>("/auth/directory")
                    .await
                    .ok()?;
            Some(
                rows.into_iter()
                    .map(|u| Mention {
                        id: u.id.to_string(),
                        display: u.name,
                        handle: u.handle,
                    })
                    .collect(),
            )
        }
        #[cfg(not(feature = "web"))]
        None
    })
}

/// Install one delegated `click` listener on the rendered markdown container.
///
/// It serves two things, because both are raw HTML injected with
/// `dangerous_inner_html` and neither can carry a Dioxus handler: a task-list
/// checkbox reports its `data-ti` index back to the caller, and a mention chip
/// reports its `data-mention` id and routes.
///
/// A mention chip is a `span`, never an `a`. A real `href` inside injected HTML
/// leaves the SPA router and reloads the whole WASM bundle, so routing goes
/// through the navigator instead.
///
/// MAPPS-586: both callbacks are `EventHandler`s, and that is load-bearing
/// rather than stylistic. This closure runs from a raw DOM listener, so no
/// dioxus scope is on the stack; anything that asks the runtime which scope it
/// is in panics inside `Runtime::current_scope_id`, which unwraps an empty
/// stack WHILE holding a shared borrow of it. Release builds are
/// `panic = "abort"`, so nothing unwinds, the borrow guard never drops, and
/// every render afterwards panics on `scope_stack.borrow_mut()`. One click
/// killed the page for good. `EventHandler::call` pushes its origin scope
/// first, which is exactly what the bare boxed closure it replaced did not do.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn install_click_listener(
    dom_id: String,
    on_toggle: Option<EventHandler<usize>>,
    on_mention: Option<EventHandler<()>>,
) {
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
        if let Some(go) = on_mention {
            // The chip's own text is a child node, so a click can land on it
            // rather than the span; `closest` walks up to the chip either way.
            if el.closest("[data-mention]").ok().flatten().is_some() {
                go.call(());
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
/// run under `web` with a real DOM, so no host test can drive them. What is
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
    /// dioxus scope. `navigator.push` writes the router's signals, which asks
    /// the runtime for the current scope; `Runtime::current_scope_id` unwraps
    /// an empty stack while holding a shared borrow of it, and because release
    /// builds are `panic = "abort"` that borrow guard never drops. Every later
    /// render then panics on `scope_stack.borrow_mut()`, so a single click on
    /// a mention chip left the page permanently unresponsive.
    ///
    /// `EventHandler::call` takes a `RuntimeGuard` and pushes its origin scope
    /// before running the body. The checkbox branch always went through one,
    /// which is why only mentions did this. Reproduced and fixed against a
    /// real browser; the harness is described on MAPPS-586.
    #[test]
    fn the_raw_listener_only_calls_things_that_carry_their_own_scope() {
        let code = code_only();
        assert!(
            !code.contains("Box<dyn Fn()>"),
            "a bare closure called from a DOM listener has no dioxus scope, and \
             the panic that causes is unrecoverable for the rest of the page"
        );
        assert!(
            code.contains("EventHandler::new(move |()|"),
            "the mention navigation must go through an EventHandler, which \
             pushes its origin scope before it runs"
        );
        assert!(
            code.contains("on_mention: Option<EventHandler<()>>"),
            "and the listener must accept nothing weaker"
        );
    }

    /// MAPPS-585: the container says whether a chip is clickable.
    ///
    /// The chip markup is identical for every reader, because it is built
    /// before anyone knows who is looking; only the click listener differs. So
    /// an inert chip and a live one looked the same, and the reporter clicked
    /// one that did nothing and asked what mentions were for. `mentions-open`
    /// is the one thing that separates them, and the stylesheet hangs the
    /// pointer and the hover on it, so both halves have to stay.
    #[test]
    fn the_container_marks_a_chip_that_actually_routes() {
        let code = code_only();
        assert!(
            code.contains("mentions-open"),
            "the container must mark itself when routing is wired"
        );
        assert!(
            code.contains("mention_target.is_some()"),
            "and the mark must come from whether there is a destination, not \
             from anything a reader without one also sees"
        );
        const CSS: &str = include_str!("../../input.css");
        assert!(
            CSS.contains(".prose.mentions-open") && CSS.contains("cursor: pointer"),
            "and the stylesheet must give the live chip a pointer, or the mark \
             changes nothing anyone can see"
        );
    }

    /// A chip must never be an `<a href>`. Rendered Markdown is injected with
    /// `dangerous_inner_html`, so a real href leaves the SPA router and reloads
    /// the whole WASM bundle. Routing goes through the navigator instead.
    #[test]
    fn a_mention_routes_through_the_router_not_an_href() {
        let code = code_only();
        assert!(
            code.contains("navigator.push("),
            "a mention click must route through the SPA navigator, never an href: \
             rendered Markdown is injected raw, so a real href leaves the router \
             and reloads the whole WASM bundle"
        );
        assert!(
            code.contains("[data-mention]"),
            "and find its chip by the data attribute, since the click can land \
             on the chip's text node rather than the span"
        );
    }

    /// The chip is only clickable for a viewer who can open the destination.
    /// `/admin/team` is admin-gated and there is no per-user page, so everyone
    /// else gets an informative chip rather than a link that 403s.
    #[test]
    fn only_a_viewer_who_can_open_the_roster_gets_a_destination() {
        let code = code_only();
        assert!(
            code.contains("UserRole::Admin") && code.contains("UserRole::SuperAdmin"),
            "the destination is gated on the roles that can actually open it"
        );
        assert!(
            code.contains("then_some(crate::Route::Team {})"),
            "and resolves to no destination at all otherwise, rather than to a \
             route the viewer cannot open"
        );
    }

    /// A failed directory load is not an error state: it yields an empty
    /// directory and every `@` renders as the plain text it already was.
    /// Nothing about a mention is worth blocking an article over.
    #[test]
    fn a_failed_directory_load_degrades_to_plain_text() {
        let code = code_only();
        assert!(
            code.contains(".flatten() .unwrap_or_default()"),
            "an absent or failed directory must collapse to an empty list, not \
             block rendering"
        );
        assert!(
            code.contains(".await .ok()?"),
            "the fetch swallows its error into `None` rather than surfacing an \
             error state for something the reader cannot act on"
        );
    }

    /// PMS-921: mentions resolve against the staff directory, not against user
    /// management. `/auth/users` is `RequireManager`, so resolving there meant
    /// a Technician saw every mention as plain text, which is the reader the
    /// feature is for. Pinned because the two paths look interchangeable and
    /// only one of them works for the audience.
    #[test]
    fn the_directory_is_the_source_not_user_management() {
        let code = code_only();
        assert!(
            code.contains("\"/auth/directory\""),
            "mentions resolve against the unprivileged staff directory"
        );
        assert!(
            !code.contains("\"/auth/users\""),
            "and never against the manager-gated user-management list, which a \
             Technician cannot read"
        );
    }
}

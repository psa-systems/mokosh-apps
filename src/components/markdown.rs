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
                // The navigation is passed as a closure rather than as a
                // navigator plus a route, so the listener needs no router type
                // in scope and stays a plain DOM concern.
                mention_route.clone().map(|route| {
                    Box::new(move || {
                        // The router reports an external-navigation failure,
                        // which cannot happen for an in-app route.
                        let _ = navigator.push(route.clone());
                    }) as Box<dyn Fn()>
                }),
            );
            #[cfg(not(target_arch = "wasm32"))]
            let _ = (&dom_id, on_toggle, &mention_route, navigator);
        });
    }
    #[cfg(not(feature = "web"))]
    let _ = &mention_target;

    rsx! {
        div {
            id: "{dom_id}",
            class: "prose dark:prose-invert max-w-none {props.class}",
            dangerous_inner_html: html,
        }
    }
}

/// The tenant's staff directory, for resolving `@handle` (MAPPS-578).
///
/// A `use_resource` per component instance, but Dioxus dedupes the underlying
/// request the same way every other `/auth/users` reader in the app relies on,
/// and the cost of being wrong here is one extra GET of a list the page has
/// usually already loaded.
///
/// A failure is not an error state: it yields an empty directory, every `@`
/// renders as the plain text it already was, and the article is unchanged from
/// what shipped before mentions existed. That is the right degrade, because
/// `GET /auth/users` is `RequireManager` on the server, so a Technician, who is
/// the typical KB reader, gets a 403 here by design.
fn use_mention_directory(enabled: bool) -> Resource<Option<Vec<Mention>>> {
    use_resource(move || async move {
        if !enabled {
            return None;
        }
        #[cfg(feature = "web")]
        {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            #[derive(serde::Deserialize)]
            struct DirectoryUser {
                id: uuid::Uuid,
                #[serde(default)]
                email: String,
                #[serde(default)]
                full_name: String,
                #[serde(default)]
                first_name: String,
                #[serde(default)]
                last_name: String,
                #[serde(default)]
                status: String,
            }
            let rows = crate::hooks::fetch::api::get_all_authed::<DirectoryUser>("/auth/users")
                .await
                .ok()?;
            Some(
                rows.into_iter()
                    // A deactivated colleague is still the right answer for a
                    // mention written while they were here, so status is not
                    // filtered on. It is read only to keep the field from being
                    // silently dropped if that decision is revisited.
                    .map(|u| {
                        let _ = &u.status;
                        let display = if u.full_name.trim().is_empty() {
                            format!("{} {}", u.first_name.trim(), u.last_name.trim())
                                .trim()
                                .to_string()
                        } else {
                            u.full_name.trim().to_string()
                        };
                        Mention {
                            id: u.id.to_string(),
                            display: if display.is_empty() {
                                u.email.clone()
                            } else {
                                display
                            },
                            email: u.email,
                        }
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
#[cfg(all(feature = "web", target_arch = "wasm32"))]
fn install_click_listener(
    dom_id: String,
    on_toggle: Option<EventHandler<usize>>,
    on_mention: Option<Box<dyn Fn()>>,
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
        if let Some(go) = on_mention.as_ref() {
            // The chip's own text is a child node, so a click can land on it
            // rather than the span; `closest` walks up to the chip either way.
            if el.closest("[data-mention]").ok().flatten().is_some() {
                go();
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

    /// A failed directory load is not an error state. It yields an empty
    /// directory, every `@` renders as the plain text it already was, and the
    /// article matches what shipped before mentions existed. This matters
    /// because `GET /auth/users` is manager-gated on the server, so the typical
    /// KB reader gets a 403 here by design.
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
}

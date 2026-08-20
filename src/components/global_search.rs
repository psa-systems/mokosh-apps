//! MAPPS-298: top-bar global search.
//!
//! A search box that lives in the TopBar between the page title and
//! the action chips. Each keystroke (debounced ~250ms via signal
//! coalescing) fires `GET /api/v1/search?q=...` and renders a grouped
//! dropdown of the top matches across tickets / contacts / companies
//! / assets / projects. Selecting a row navigates to the entity's
//! detail page.
//!
//! The server is the source of truth for what's searchable - this
//! component only concerns itself with the input, the network call,
//! and the dropdown render. See `mokosh-server/src/modules/search/`
//! for the SQL.

#[cfg(feature = "web")]
use crate::components::{ErrorBanner, Input};
use crate::hooks::{use_dropdown_nav, NavAction};
use crate::utils::url::urlencoding_minimal;
use crate::Route;
use dioxus::prelude::*;
use serde::Deserialize;

/// Mirror of the server's `SearchResponse` envelope.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
struct SearchResponse {
    #[serde(default)]
    tickets: Vec<SearchHit>,
    #[serde(default)]
    contacts: Vec<SearchHit>,
    #[serde(default)]
    companies: Vec<SearchHit>,
    #[serde(default)]
    assets: Vec<SearchHit>,
    #[serde(default)]
    projects: Vec<SearchHit>,
    #[serde(default)]
    counts: SearchCounts,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
struct SearchCounts {
    #[serde(default)]
    tickets: i64,
    #[serde(default)]
    contacts: i64,
    #[serde(default)]
    companies: i64,
    #[serde(default)]
    assets: i64,
    #[serde(default)]
    projects: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct SearchHit {
    id: uuid::Uuid,
    label: String,
    #[serde(default)]
    secondary: Option<String>,
}

#[component]
pub fn GlobalSearch() -> Element {
    let mut query = use_signal(String::new);
    // MAPPS-503: open / highlight state and the shared keyboard contract.
    let mut nav = use_dropdown_nav("global-search");
    // MAPPS-346: collapsed to a magnifier icon by default; the icon lives in
    // the top-bar action cluster (left of the theme picker) and expands the
    // text entry leftward on click.
    let mut expanded = use_signal(|| false);
    let navigator = use_navigator();

    // MAPPS-347: focus the entry as soon as it expands, without touching the
    // shared `Input` component. Running the effect at top level (Dioxus hook
    // ordering) and gating the focus call on `expanded()` keeps this local to
    // the search component. The effect fires after commit, so the input is
    // mounted by the time we `getElementById` it.
    #[cfg(feature = "web")]
    use_effect(move || {
        if expanded() {
            // MAPPS-503 made this failure loud; MAPPS-504 moved the call
            // itself behind the platform boundary, which is where the
            // logging now lives so every focus site gets it.
            crate::platform::dom::focus_by_id("global_search");
        }
    });

    // Reading the query signal inside the resource closure subscribes
    // the resource to it so each keystroke re-fetches (same pattern as
    // CompanyPicker / ContactPicker). The empty / very-short branches
    // short-circuit without hitting the network.
    let results = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let q = query.read().trim().to_string();
        if q.len() < 2 {
            // Two-char minimum keeps the dropdown out of "I just clicked the
            // field" noise. The server tolerates short queries but the SPA
            // doesn't benefit from showing dozens of matches on a single
            // letter.
            return Ok(SearchResponse::default());
        }
        let path = format!("/search?q={}", urlencoding_minimal(&q));
        // MAPPS-503: keep the failure. `unwrap_or_default()` here turned a
        // failed search into "no matches", with nothing logged.
        crate::hooks::fetch::api::get_authed::<SearchResponse>(&path)
            .await
            .inspect_err(|e| tracing::warn!("global search failed: {e}"))
    });

    let snap = results.read_unchecked();
    let failed = matches!(&*snap, Some(Err(_)));
    let response = match &*snap {
        Some(Ok(r)) => r.clone(),
        _ => SearchResponse::default(),
    };
    let query_text = query.read().clone();
    let any_results = !response.tickets.is_empty()
        || !response.contacts.is_empty()
        || !response.companies.is_empty()
        || !response.assets.is_empty()
        || !response.projects.is_empty();

    // MAPPS-503: the grouped panel is one flat list for the keyboard. Each
    // section knows where it starts, and the route for row `i` is
    // `targets[i]`, in the same order the sections render.
    let section_offsets = [
        0,
        response.tickets.len(),
        response.tickets.len() + response.contacts.len(),
        response.tickets.len() + response.contacts.len() + response.companies.len(),
        response.tickets.len()
            + response.contacts.len()
            + response.companies.len()
            + response.assets.len(),
    ];
    let targets: Vec<Route> = response
        .tickets
        .iter()
        .map(|h| Route::TicketDetail {
            id: h.id.to_string(),
        })
        .chain(response.contacts.iter().map(|h| Route::ContactDetail {
            id: h.id.to_string(),
        }))
        .chain(response.companies.iter().map(|h| Route::CompanyDetail {
            id: h.id.to_string(),
        }))
        .chain(response.assets.iter().map(|h| Route::AssetDetail {
            id: h.id.to_string(),
        }))
        .chain(response.projects.iter().map(|h| Route::ProjectDetail {
            id: h.id.to_string(),
        }))
        .collect();
    let nav_len = targets.len();
    let active = nav.active_index();
    let id_prefix = nav.row_id_prefix();

    rsx! {
        div { class: "relative flex items-center",
            // MAPPS-346: outside-click backdrop while expanded. Closes the
            // dropdown; collapses to the icon only when the entry is empty,
            // so a typed query is never lost by an accidental outside click.
            if expanded() {
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| {
                        nav.close();
                        if query.read().trim().is_empty() {
                            expanded.set(false);
                        }
                    },
                }
            }
            // The text entry expands leftward from the fixed icon. Absolute
            // `right-full` anchors its right edge to the icon so neither the
            // icon nor the sibling action chips shift when it opens.
            if expanded() {
                div { class: "absolute right-full top-1/2 -translate-y-1/2 mr-2 z-20 w-72",
                    // MAPPS-347: the keydown handler lives on the wrapper div,
                    // so keydown bubbles up from the focused input to here.
                    // This keeps the shared `Input` component free of extra
                    // event handlers (they interfered with inline-error
                    // rendering on the ticket-create form).
                    div {
                        class: "relative",
                        role: "combobox",
                        aria_expanded: nav.expanded(),
                        aria_controls: nav.panel_id(),
                        aria_activedescendant: nav.active_descendant(),
                        onkeydown: move |e: KeyboardEvent| {
                            let targets = targets.clone();
                            let action = nav
                                .keydown(
                                    &e,
                                    nav_len,
                                    move |index| {
                                        if let Some(route) = targets.get(index) {
                                            query.set(String::new());
                                            navigator.push(route.clone());
                                        }
                                    },
                                );
                            // MAPPS-347: Escape also drops the typed text and
                            // collapses the entry back to the icon, which is
                            // more than the shared hook's "close the list".
                            if action == NavAction::Close {
                                query.set(String::new());
                                expanded.set(false);
                            }
                        },
                        // Only the field opens the list on click / focus. The
                        // panel is a sibling of this div, so picking a row
                        // cannot bubble back in here and re-open what it just
                        // closed.
                        div {
                            class: "relative",
                            onfocusin: move |_| nav.open(),
                            onclick: move |_| nav.open(),
                            div { class: "absolute inset-y-0 left-0 flex items-center pl-3 pointer-events-none text-subtle",
                                // Inline magnifier glyph - the icons module doesn't
                                // ship a search icon and this is the only consumer.
                                svg {
                                    class: "w-4 h-4",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M21 21l-4.35-4.35M11 19a8 8 0 100-16 8 8 0 000 16z",
                                    }
                                }
                            }
                            Input {
                                name: "global_search",
                                label: "".to_string(),
                                // MAPPS-314: visually-hidden label via aria-label;
                                // a placeholder alone leaves screen-reader users
                                // with nothing once typing begins.
                                aria_label: "Search tickets, contacts, companies, assets, projects".to_string(),
                                placeholder: "Search tickets, contacts, companies…",
                                value: query.read().clone(),
                                class: "pl-9".to_string(),
                                oninput: move |e: FormEvent| {
                                    query.set(e.value());
                                    nav.open_fresh();
                                },
                            }
                        }
                        if nav.is_open() {
                            div {
                                id: nav.panel_id(),
                                role: "listbox",
                                class: "dropdown-panel absolute z-20 left-0 right-0 mt-1 max-h-[32rem] overflow-y-auto",
                                if query_text.trim().len() < 2 {
                                    div { class: "px-3 py-2 text-sm text-muted",
                                        "Type at least two characters to search."
                                    }
                                } else if failed {
                                    // MAPPS-503: a failed search is its own
                                    // state, not "no matches".
                                    ErrorBanner { class: "m-1", "Could not search. Try again." }
                                } else if !any_results {
                                    div { class: "px-3 py-2 text-sm text-muted",
                                        "No matches across tickets, contacts, companies, assets, or projects."
                                    }
                                } else {
                                    SearchSection {
                                        label: "Tickets".to_string(),
                                        total: response.counts.tickets,
                                        hits: response.tickets.clone(),
                                        offset: section_offsets[0],
                                        active,
                                        id_prefix: id_prefix.clone(),
                                        onpick: move |id: String| {
                                            nav.close();
                                            query.set(String::new());
                                            navigator.push(Route::TicketDetail { id });
                                        },
                                    }
                                    SearchSection {
                                        label: "Contacts".to_string(),
                                        total: response.counts.contacts,
                                        hits: response.contacts.clone(),
                                        offset: section_offsets[1],
                                        active,
                                        id_prefix: id_prefix.clone(),
                                        onpick: move |id: String| {
                                            nav.close();
                                            query.set(String::new());
                                            navigator.push(Route::ContactDetail { id });
                                        },
                                    }
                                    SearchSection {
                                        label: "Companies".to_string(),
                                        total: response.counts.companies,
                                        hits: response.companies.clone(),
                                        offset: section_offsets[2],
                                        active,
                                        id_prefix: id_prefix.clone(),
                                        onpick: move |id: String| {
                                            nav.close();
                                            query.set(String::new());
                                            navigator.push(Route::CompanyDetail { id });
                                        },
                                    }
                                    SearchSection {
                                        label: "Assets".to_string(),
                                        total: response.counts.assets,
                                        hits: response.assets.clone(),
                                        offset: section_offsets[3],
                                        active,
                                        id_prefix: id_prefix.clone(),
                                        onpick: move |id: String| {
                                            nav.close();
                                            query.set(String::new());
                                            navigator.push(Route::AssetDetail { id });
                                        },
                                    }
                                    SearchSection {
                                        label: "Projects".to_string(),
                                        total: response.counts.projects,
                                        hits: response.projects.clone(),
                                        offset: section_offsets[4],
                                        active,
                                        id_prefix: id_prefix.clone(),
                                        onpick: move |id: String| {
                                            nav.close();
                                            query.set(String::new());
                                            navigator.push(Route::ProjectDetail { id });
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // MAPPS-346: the magnifier icon stays fixed in the action
            // cluster. Clicking opens + focuses the entry; it sits above
            // the backdrop (z-30) so it stays clickable while expanded.
            button {
                r#type: "button",
                class: "relative z-30 p-2 rounded-full text-subtle hover:text-content hover:bg-surface-2",
                aria_label: "Search",
                title: "Search",
                aria_expanded: if expanded() { "true" } else { "false" },
                onclick: move |_| expanded.set(true),
                svg {
                    class: "w-5 h-5",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M21 21l-4.35-4.35M11 19a8 8 0 100-16 8 8 0 000 16z",
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SearchSectionProps {
    label: String,
    total: i64,
    hits: Vec<SearchHit>,
    /// MAPPS-503: where this section's hits start in the panel's flat
    /// keyboard list, so a row knows its own navigable index.
    offset: usize,
    /// The highlighted row's flat index, if any.
    active: Option<usize>,
    /// Row-id prefix from `DropdownNav`, so `aria-activedescendant` on the
    /// field wrapper resolves to a row this section renders.
    id_prefix: String,
    onpick: EventHandler<String>,
}

#[component]
fn SearchSection(props: SearchSectionProps) -> Element {
    if props.hits.is_empty() {
        return rsx! {};
    }
    let total = props.total;
    let count_hint = if total > props.hits.len() as i64 {
        format!("{}/{}", props.hits.len(), total)
    } else {
        format!("{}", total)
    };
    rsx! {
        div { class: "py-1",
            div { class: "px-3 py-1 flex items-center justify-between text-xs font-semibold uppercase tracking-wide text-subtle bg-surface-2",
                span { "{props.label}" }
                span { "{count_hint}" }
            }
            // MAPPS-503: `role="none"` so the rows stay the listbox panel's
            // own options across the section grouping.
            ul { role: "none",
                for (i , hit) in props.hits.into_iter().enumerate() {
                    {
                        let id_str = hit.id.to_string();
                        let key = id_str.clone();
                        let id_for_click = id_str.clone();
                        let onpick = props.onpick;
                        let index = props.offset + i;
                        let row_id = format!("{}{index}", props.id_prefix);
                        let is_active = props.active == Some(index);
                        let row_class = if is_active {
                            "w-full text-left px-3 py-2 text-sm hover:bg-surface-2 bg-surface-2"
                        } else {
                            "w-full text-left px-3 py-2 text-sm hover:bg-surface-2"
                        };
                        rsx! {
                            li {
                                key: "{key}",
                                id: "{row_id}",
                                role: "option",
                                aria_selected: if is_active { "true" } else { "false" },
                                button {
                                    r#type: "button",
                                    // MAPPS-503: out of the tab order, so Tab commits
                                    // and moves on instead of walking into the list.
                                    tabindex: "-1",
                                    class: "{row_class}",
                                    onclick: move |_| onpick.call(id_for_click.clone()),
                                    div { class: "font-medium text-content", "{hit.label}" }
                                    if let Some(secondary) = hit.secondary {
                                        if !secondary.trim().is_empty() {
                                            div { class: "text-xs text-muted", "{secondary}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

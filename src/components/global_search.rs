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

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::Input;
use crate::utils::url::urlencoding_minimal;
use crate::Route;

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
    let mut show_dropdown = use_signal(|| false);
    // MAPPS-346: collapsed to a magnifier icon by default; the icon lives in
    // the top-bar action cluster (left of the theme picker) and expands the
    // text entry leftward on click.
    let mut expanded = use_signal(|| false);
    let navigator = use_navigator();

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
            return SearchResponse::default();
        }
        let path = format!("/search?q={}", urlencoding_minimal(&q));
        crate::hooks::fetch::api::get_authed::<SearchResponse>(&path)
            .await
            .unwrap_or_default()
    });

    let snap = results.read_unchecked();
    let response = snap.clone().unwrap_or_default();
    let query_text = query.read().clone();
    let any_results = !response.tickets.is_empty()
        || !response.contacts.is_empty()
        || !response.companies.is_empty()
        || !response.assets.is_empty()
        || !response.projects.is_empty();

    rsx! {
        div { class: "relative flex items-center",
            // MAPPS-346: outside-click backdrop while expanded. Closes the
            // dropdown; collapses to the icon only when the entry is empty,
            // so a typed query is never lost by an accidental outside click.
            if expanded() {
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| {
                        show_dropdown.set(false);
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
                    div { class: "relative",
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
                    placeholder: "Search tickets, contacts, companies...",
                    value: query.read().clone(),
                    class: "pl-9".to_string(),
                    // MAPPS-346: ready to type the moment the icon expands.
                    autofocus: true,
                    oninput: move |e: FormEvent| {
                        query.set(e.value());
                        show_dropdown.set(true);
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        // Escape is an explicit cancel: clear and collapse.
                        if e.key() == Key::Escape {
                            query.set(String::new());
                            show_dropdown.set(false);
                            expanded.set(false);
                        }
                    },
                }
            }
            if *show_dropdown.read() && !query_text.trim().is_empty() {
                div { class: "absolute z-20 left-0 right-0 mt-1 max-h-[32rem] overflow-y-auto rounded-md border border-line bg-raised shadow-lg",
                    if query_text.trim().len() < 2 {
                        div { class: "px-3 py-2 text-sm text-muted",
                            "Type at least two characters to search."
                        }
                    } else if !any_results {
                        div { class: "px-3 py-2 text-sm text-muted",
                            "No matches across tickets, contacts, companies, assets, or projects."
                        }
                    } else {
                        SearchSection {
                            label: "Tickets".to_string(),
                            total: response.counts.tickets,
                            hits: response.tickets.clone(),
                            onpick: {
                                let nav = navigator;
                                move |id: String| {
                                    show_dropdown.set(false);
                                    query.set(String::new());
                                    nav.push(Route::TicketDetail { id });
                                }
                            },
                        }
                        SearchSection {
                            label: "Contacts".to_string(),
                            total: response.counts.contacts,
                            hits: response.contacts.clone(),
                            onpick: {
                                let nav = navigator;
                                move |id: String| {
                                    show_dropdown.set(false);
                                    query.set(String::new());
                                    nav.push(Route::ContactDetail { id });
                                }
                            },
                        }
                        SearchSection {
                            label: "Companies".to_string(),
                            total: response.counts.companies,
                            hits: response.companies.clone(),
                            onpick: {
                                let nav = navigator;
                                move |id: String| {
                                    show_dropdown.set(false);
                                    query.set(String::new());
                                    nav.push(Route::CompanyDetail { id });
                                }
                            },
                        }
                        SearchSection {
                            label: "Assets".to_string(),
                            total: response.counts.assets,
                            hits: response.assets.clone(),
                            onpick: {
                                let nav = navigator;
                                move |id: String| {
                                    show_dropdown.set(false);
                                    query.set(String::new());
                                    nav.push(Route::AssetDetail { id });
                                }
                            },
                        }
                        SearchSection {
                            label: "Projects".to_string(),
                            total: response.counts.projects,
                            hits: response.projects.clone(),
                            onpick: {
                                let nav = navigator;
                                move |id: String| {
                                    show_dropdown.set(false);
                                    query.set(String::new());
                                    nav.push(Route::ProjectDetail { id });
                                }
                            },
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
                class: "relative z-30 p-2 rounded-full text-gray-400 hover:text-white hover:bg-gray-700",
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
            ul {
                for hit in props.hits.into_iter() {
                    {
                        let id_str = hit.id.to_string();
                        let key = id_str.clone();
                        let id_for_click = id_str.clone();
                        let onpick = props.onpick;
                        rsx! {
                            li { key: "{key}",
                                button {
                                    r#type: "button",
                                    class: "w-full text-left px-3 py-2 text-sm hover:bg-surface-2",
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

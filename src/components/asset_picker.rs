//! Reusable Asset picker (PMS-344).
//!
//! Mirror of [`crate::components::CompanyPicker`]: hits
//! `GET /assets?q=...&per_page=20` on each keystroke, renders the
//! matches in a click-to-select dropdown, and reports the selected
//! asset's UUID + name back through callbacks. Used on the New Ticket
//! form and on the ticket-detail inline-edit sidebar to associate a
//! ticket with an asset.
//!
//! Stays narrow on purpose: no `allow_inline_create` (creating an asset
//! is heavier than creating a company and is best done from the Assets
//! page itself), no per-page server-side scope to `company_id` (the
//! ticket form filters by company elsewhere; pre-filtering here would
//! constrain the picker awkwardly if the user changes the ticket's
//! company after picking an asset).

use dioxus::prelude::*;
use serde::Deserialize;

use crate::components::{Button, ButtonSize, ButtonVariant, ErrorBanner, Input};
use crate::utils::url::urlencoding_minimal;

#[derive(Clone, Debug, Deserialize)]
struct PickerAsset {
    id: uuid::Uuid,
    name: String,
    #[serde(default)]
    asset_tag: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PickerPage {
    data: Vec<PickerAsset>,
}

#[derive(Props, Clone, PartialEq)]
pub struct AssetPickerProps {
    /// Currently selected asset name (or empty if none selected).
    pub value: String,
    /// Optional currently selected id. When `Some` the picker renders
    /// as a "selected chip with a Change button" instead of the
    /// search dropdown - matches the CompanyPicker shape.
    pub selected_id: Option<String>,
    /// Field label rendered above the input.
    #[props(default = String::from("Asset"))]
    pub label: String,
    /// Mark the underlying input required. Defaults to false because
    /// most tickets have no asset; the field is opt-in per ticket.
    #[props(default)]
    pub required: bool,
    /// Fires once when the user picks a row. Receives `(id, name)`.
    pub onselect: EventHandler<(String, String)>,
    /// Fires when the user clears the selection (Change / X button).
    pub onclear: EventHandler<()>,
}

#[component]
pub fn AssetPicker(props: AssetPickerProps) -> Element {
    let mut query = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);
    // PMS-344 follow-up: when the picker has a selected_id from the
    // parent (e.g. the inline ticket-detail editor showing the currently
    // associated asset), clicking Change must surface the search input
    // immediately - WITHOUT firing onclear. The previous implementation
    // cleared the association server-side via onclear and waited for the
    // refetched ticket to re-render with selected_id = None before the
    // search input appeared; that round trip is invisible to the user,
    // so they read it as "Change does nothing". The internal `editing`
    // signal lets the picker swap to search mode locally without a
    // network call. Selection in search mode then fires the standard
    // onselect with the new asset and exits editing.
    let mut editing = use_signal(|| false);

    // PMS-371: read the query signal INSIDE the use_resource closure so
    // Dioxus subscribes the resource to it (same pattern as the
    // CompanyPicker fix). Reading outside the closure only subscribes
    // the parent component, leaving the fetch firing once with the
    // initial empty query and ignoring subsequent keystrokes.
    let query_text = query.read().trim().to_string();
    let results = use_resource(move || async move {
        let _gen = crate::hooks::fetch::active_tenant_generation();
        let q = query.read().trim().to_string();
        let path = if q.is_empty() {
            "/assets?per_page=20".to_string()
        } else {
            format!("/assets?q={}&per_page=20", urlencoding_minimal(&q))
        };
        crate::hooks::fetch::api::get_authed::<PickerPage>(&path)
            .await
            .ok()
            .map(|p| p.data)
    });

    // Chip view: parent supplied a selected_id AND the user is not
    // currently in inline-edit mode. Change toggles `editing` so the
    // search input mounts on the next render with no server round trip.
    if let Some(_id) = &props.selected_id {
        if !editing() {
            let name = props.value.clone();
            let onclear = props.onclear;
            // PMS-344 follow-up (layout): skip the picker's own label
            // when the parent passed an empty string. The inline ticket-
            // detail editor wraps the picker in a `DetailItem` that
            // already renders "Asset" on the left side of the row, so
            // letting the picker render a second label produced a
            // duplicated "Asset" inside the value column. Same convention
            // the inline Status/Priority/Assigned-To `Select` editors
            // already follow (they pass `label: ""`).
            let show_label = !props.label.trim().is_empty();
            return rsx! {
                div { class: "space-y-1 w-full",
                    if show_label {
                        label { class: "block text-sm font-medium text-content",
                            "{props.label}"
                            if props.required {
                                span { class: "text-red-500 dark:text-red-400 ml-0.5", "*" }
                            }
                        }
                    }
                    div {
                        // w-full + min-w-0 so the chip fills its parent
                        // (the DetailItem `dd` cell when used inline)
                        // instead of escaping rightward, and the long
                        // name truncates cleanly inside. MAPPS-273:
                        // dropped the raw UUID secondary line.
                        class: "flex items-center justify-between border border-line rounded-md px-3 py-2 bg-app w-full min-w-0",
                        div { class: "min-w-0 flex-1 text-left",
                            p { class: "text-sm font-medium text-content truncate", "{name}" }
                        }
                        div { class: "flex items-center gap-1 shrink-0 ml-2",
                            Button {
                                variant: ButtonVariant::Link,
                                size: ButtonSize::Small,
                                onclick: move |_| {
                                    query.set(String::new());
                                    editing.set(true);
                                    show_dropdown.set(true);
                                },
                                "Change"
                            }
                            // PMS-344 follow-up: explicit "Unassign"
                            // affordance so the user can drop the asset
                            // association without picking a replacement.
                            // Fires the parent's onclear (which PUTs
                            // asset_id = null on a ticket inline editor).
                            button {
                                r#type: "button",
                                class: "text-xs text-muted hover:text-red-600 dark:hover:text-red-400 px-2 py-1",
                                onclick: move |_| {
                                    onclear.call(());
                                    query.set(String::new());
                                },
                                "Unassign"
                            }
                        }
                    }
                }
            };
        }
    }

    let snap = results.read_unchecked();
    let onselect = props.onselect;
    rsx! {
        div { class: "relative space-y-1",
            Input {
                name: "asset_search",
                label: props.label,
                placeholder: "Search assets…",
                required: props.required,
                value: query.read().clone(),
                oninput: move |e: FormEvent| {
                    query.set(e.value());
                    show_dropdown.set(true);
                },
            }
            if *show_dropdown.read() {
                // Backdrop dismisses the dropdown on click-outside; sits
                // below the dropdown's z-20 so rows stay clickable. Also
                // exits inline-edit mode so the chip view returns when
                // the user cancels by clicking outside without picking.
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| {
                        show_dropdown.set(false);
                        editing.set(false);
                    },
                }
                div {
                    class: "dropdown-panel absolute z-20 left-0 right-0 mt-1 max-h-72 overflow-y-auto",
                    match &*snap {
                        None => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted", "Searching…" }
                        },
                        Some(None) => rsx! {
                            // MAPPS-444: the only signal the list failed, so it
                            // takes the shared banner (paired hues, role="alert").
                            // `m-1` keeps its border off the dropdown's own edge.
                            ErrorBanner { class: "m-1", "Could not load assets." }
                        },
                        Some(Some(rows)) if rows.is_empty() => rsx! {
                            div { class: "px-3 py-2 text-sm text-muted",
                                if query_text.is_empty() {
                                    "No assets yet."
                                } else {
                                    "No matches."
                                }
                            }
                        },
                        Some(Some(rows)) => {
                            let rows = rows.clone();
                            rsx! {
                                ul { class: "py-1",
                                    for row in rows.into_iter() {
                                        {
                                            let id_str = row.id.to_string();
                                            let key = id_str.clone();
                                            let name = row.name.clone();
                                            let tag = row
                                                .asset_tag
                                                .clone()
                                                .filter(|s| !s.trim().is_empty());
                                            let id_for_click = id_str.clone();
                                            let name_for_click = name.clone();
                                            rsx! {
                                                li {
                                                    key: "{key}",
                                                    button {
                                                        r#type: "button",
                                                        class: "w-full text-left px-3 py-2 text-sm hover:bg-surface-2",
                                                        onclick: move |_| {
                                                            onselect.call((id_for_click.clone(), name_for_click.clone()));
                                                            show_dropdown.set(false);
                                                            editing.set(false);
                                                            query.set(name_for_click.clone());
                                                        },
                                                        span { class: "font-medium", "{name}" }
                                                        if let Some(t) = tag {
                                                            span { class: "ml-2 text-xs text-muted font-mono", "{t}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

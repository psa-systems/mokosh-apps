//! Free-text input with suggestions (PMS-583, PMS-582).
//!
//! Unlike [`CompanyPicker`](super::CompanyPicker), this never forces a
//! selection. The field stays free text; the dropdown only offers suggestions.
//! Picking one fills the text; typing anything else is preserved.
//!
//! Two suggestion sources, picked by which prop is set:
//! - `field`: server-sourced from the tenant's own past entries via
//!   `GET /contacts/field-values?field=<field>` (Title / Department, PMS-583).
//!   Good for open-ended fields where the past data IS the vocabulary.
//! - `suggestions`: a curated static list, filtered client-side as you type
//!   (Company Industry, PMS-582). Good for a finite taxonomy where the point
//!   is to standardize toward clean canonical values, not echo existing
//!   (possibly inconsistent) data, while still allowing a free-text long tail.
//!
//! When `suggestions` is non-empty it wins and no request is made.

use dioxus::prelude::*;

use crate::components::{ErrorBanner, Input};
use crate::hooks::use_dropdown_nav;
use crate::utils::url::urlencoding_minimal;

#[derive(Props, Clone, PartialEq)]
pub struct SuggestInputProps {
    pub name: String,
    pub label: String,
    /// Server-suggestion source: which contact field to query (`"title"` /
    /// `"department"`). Ignored when `suggestions` is set. Empty = no server
    /// fetch.
    #[props(default)]
    pub field: String,
    /// Curated static suggestion list, filtered client-side by the typed text.
    /// When non-empty, this is the source and no request is made.
    #[props(default)]
    pub suggestions: Vec<String>,
    /// Current text (controlled by the parent form's signal).
    pub value: String,
    #[props(default)]
    pub required: bool,
    #[props(default)]
    pub error: String,
    #[props(default)]
    pub help: String,
    /// Fires on every text change, whether typed or picked from the list.
    pub oninput: EventHandler<String>,
}

#[component]
pub fn SuggestInput(props: SuggestInputProps) -> Element {
    // MAPPS-503: open / highlight state and the shared keyboard contract.
    let mut nav = use_dropdown_nav("suggest-input");
    // Mirror the current text for the server fetch. Read INSIDE the resource
    // closure so Dioxus subscribes the resource to keystrokes (CompanyPicker
    // PMS-371). Unused in the curated-list path, harmless there.
    let mut query = use_signal(|| props.value.clone());
    let static_mode = !props.suggestions.is_empty();
    let field = props.field.clone();

    let results = use_resource(move || {
        let field = field.clone();
        async move {
            // Curated list or no field configured: nothing to fetch.
            if static_mode || field.is_empty() {
                return Ok(Vec::new());
            }
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let q = query.read().trim().to_string();
            let path = format!(
                "/contacts/field-values?field={}&q={}",
                urlencoding_minimal(&field),
                urlencoding_minimal(&q),
            );
            // MAPPS-503: keep the failure. `.ok().unwrap_or_default()` here
            // made a failed lookup indistinguishable from "no suggestions",
            // with nothing logged.
            crate::hooks::fetch::api::get_authed::<Vec<String>>(&path)
                .await
                .inspect_err(|e| tracing::warn!("suggestion lookup failed: {e}"))
        }
    });

    let oninput = props.oninput;
    // Drop a suggestion equal to the current text: nothing to pick.
    let current = props.value.trim().to_string();
    let snap = results.read_unchecked();
    // MAPPS-503: a failed lookup is its own panel state, so it is not read as
    // "this field has no suggestions".
    let failed = !static_mode && matches!(&*snap, Some(Err(_)));
    let suggestions: Vec<String> = if static_mode {
        // Curated list, filtered client-side by case-insensitive substring.
        let needle = current.to_lowercase();
        props
            .suggestions
            .iter()
            .filter(|s| needle.is_empty() || s.to_lowercase().contains(&needle))
            .filter(|s| s.as_str() != current)
            .take(20)
            .cloned()
            .collect()
    } else {
        match &*snap {
            Some(Ok(values)) => values.iter().filter(|s| *s != &current).cloned().collect(),
            _ => Vec::new(),
        }
    };

    let nav_len = suggestions.len();
    let suggestions_for_keys = suggestions.clone();
    rsx! {
        div { class: "relative space-y-1",
            // MAPPS-503: combobox seam. The handlers live on this wrapper
            // rather than on the shared `Input` (MAPPS-347), where keydown
            // from the focused input bubbles up to them.
            div {
                role: "combobox",
                aria_expanded: nav.expanded(),
                aria_controls: nav.panel_id(),
                aria_activedescendant: nav.active_descendant(),
                onfocusin: move |_| nav.open(),
                onclick: move |_| nav.open(),
                onkeydown: move |e: KeyboardEvent| {
                    let suggestions = suggestions_for_keys.clone();
                    nav.keydown(&e, nav_len, move |index| {
                        if let Some(value) = suggestions.get(index) {
                            query.set(value.clone());
                            oninput.call(value.clone());
                        }
                    });
                },
                Input {
                    name: props.name.clone(),
                    label: props.label.clone(),
                    required: props.required,
                    error: props.error.clone(),
                    help: props.help.clone(),
                    value: props.value.clone(),
                    oninput: move |e: FormEvent| {
                        let v = e.value();
                        query.set(v.clone());
                        oninput.call(v);
                        nav.open_fresh();
                    },
                }
            }
            if nav.is_open() && (!suggestions.is_empty() || failed) {
                // Transparent backdrop: a click anywhere outside dismisses the
                // list. Below the dropdown (z-10 vs z-20) so rows stay clickable.
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| nav.close(),
                }
                div {
                    id: nav.panel_id(),
                    role: "listbox",
                    class: "dropdown-panel absolute z-20 left-0 right-0 mt-1 max-h-60 overflow-y-auto",
                    if failed {
                        // MAPPS-444: the shared banner (paired hues,
                        // role="alert"); `m-1` keeps its border off the
                        // dropdown's own edge.
                        ErrorBanner { class: "m-1", "Could not search. Try again." }
                    } else {
                        // MAPPS-503: `role="none"` so the rows stay the
                        // listbox panel's own options.
                        ul { class: "py-1", role: "none",
                            for (index , s) in suggestions.iter().enumerate() {
                                {
                                    let val = s.clone();
                                    let key = s.clone();
                                    let text = s.clone();
                                    rsx! {
                                        li {
                                            key: "{key}",
                                            id: nav.row_id(index),
                                            role: "option",
                                            aria_selected: nav.row_selected(index),
                                            button {
                                                r#type: "button",
                                                // MAPPS-503: out of the tab order, so Tab
                                                // commits and moves to the next field
                                                // instead of walking into the list.
                                                tabindex: "-1",
                                                class: nav.row_class(index, "w-full text-left px-3 py-2 text-sm hover:bg-surface-2"),
                                                onclick: move |_| {
                                                    query.set(val.clone());
                                                    oninput.call(val.clone());
                                                    nav.close();
                                                },
                                                "{text}"
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
}

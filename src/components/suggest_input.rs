//! Free-text input with server-sourced suggestions (PMS-583).
//!
//! Unlike [`CompanyPicker`](super::CompanyPicker), this never forces a
//! selection. The field stays free text; the dropdown only offers values the
//! tenant has already entered for this field, fetched from
//! `GET /contacts/field-values?field=<field>&q=<text>`. Picking a suggestion
//! fills the text; typing anything else is preserved. Suggestions are
//! server-sourced (shared across the tenant's users and devices), not the
//! browser's autofill cache.
//!
//! Used by the contact form's Title and Department fields, which are
//! deliberately open-ended (job titles especially) so a closed dropdown is the
//! wrong tool; the suggestion list nudges consistency without blocking new
//! values.

use dioxus::prelude::*;

use crate::components::Input;
use crate::utils::url::urlencoding_minimal;

#[derive(Props, Clone, PartialEq)]
pub struct SuggestInputProps {
    pub name: String,
    pub label: String,
    /// Which contact field to query for suggestions: `"title"` or
    /// `"department"`. Passed straight to the endpoint's `field` param, which
    /// the server validates against its closed enum.
    pub field: String,
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
    let mut show_dropdown = use_signal(|| false);
    // Mirror the current text for the fetch. Read INSIDE the resource closure
    // so Dioxus subscribes the resource to keystrokes (CompanyPicker PMS-371).
    let mut query = use_signal(|| props.value.clone());
    let field = props.field.clone();

    let results = use_resource(move || {
        let field = field.clone();
        async move {
            let _gen = crate::hooks::fetch::active_tenant_generation();
            let q = query.read().trim().to_string();
            let path = format!(
                "/contacts/field-values?field={}&q={}",
                urlencoding_minimal(&field),
                urlencoding_minimal(&q),
            );
            crate::hooks::fetch::api::get_authed::<Vec<String>>(&path)
                .await
                .ok()
                .unwrap_or_default()
        }
    });

    let oninput = props.oninput;
    let snap = results.read_unchecked();
    // Drop a suggestion that exactly equals the current text: it offers
    // nothing to pick and would otherwise sit highlighted under the cursor.
    let current = props.value.trim().to_string();
    let suggestions: Vec<String> = snap
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s != &current)
        .collect();

    rsx! {
        div { class: "relative space-y-1",
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
                    show_dropdown.set(true);
                },
            }
            if *show_dropdown.read() && !suggestions.is_empty() {
                // Transparent backdrop: a click anywhere outside dismisses the
                // list. Below the dropdown (z-10 vs z-20) so rows stay clickable.
                div {
                    class: "fixed inset-0 z-10",
                    onclick: move |_| show_dropdown.set(false),
                }
                div {
                    class: "absolute z-20 left-0 right-0 mt-1 max-h-60 overflow-y-auto rounded-md border border-line bg-raised shadow-lg",
                    ul { class: "py-1",
                        for s in suggestions.into_iter() {
                            {
                                let val = s.clone();
                                let key = s.clone();
                                rsx! {
                                    li {
                                        key: "{key}",
                                        button {
                                            r#type: "button",
                                            class: "w-full text-left px-3 py-2 text-sm hover:bg-surface-2",
                                            onclick: move |_| {
                                                query.set(val.clone());
                                                oninput.call(val.clone());
                                                show_dropdown.set(false);
                                            },
                                            "{s}"
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

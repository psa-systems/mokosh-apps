//! Reader side-panel for a KB article's declared template variables.
//!
//! An article may declare `template_variables`: named placeholders a
//! reader fills in to get a specific rendering. The server owns
//! substitution via `POST /api/v1/kb/articles/{id}/render`; this panel
//! collects the values and asks for a fresh render on every change.
//!
//! Shape:
//! - one row per variable, ordered as the article declared them;
//! - a variable with a non-empty `allowed_values` renders as a `<select>`;
//!   otherwise a free-text `<input>`;
//! - each value seeds from the variable's `default` (empty when none);
//! - a 300ms debounce coalesces a burst of keystrokes into one render;
//! - one in-flight render at a time, cancelled and reissued on a change;
//! - a 422 shows a one-line error above the panel, the previous rendered
//!   body stays, the pickers themselves are not blocked.
//!
//! The panel does NOT render when the article declares no variables; the
//! caller is responsible for not mounting it in that case so the page is
//! unchanged for the non-templated article.

use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::modules::kb::KbTemplateVariable;

/// Build the request body for `POST /kb/articles/{id}/render` from a
/// reader's picks.
///
/// Only variables the article actually declares are forwarded, so a stale
/// signal entry (one left after a variable was renamed server-side)
/// cannot reach the service and trip the 400 "unknown name" check. An
/// empty value is dropped: an unset variable should render as the
/// server's `[missing: NAME]` placeholder, not as the empty string.
///
/// Returned ordered (BTreeMap), so the serialized body is stable across
/// renders and a cached response can be matched by request body.
pub fn render_values(
    declared: &[KbTemplateVariable],
    picks: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for v in declared {
        if let Some(value) = picks.get(&v.name) {
            if !value.is_empty() {
                out.insert(v.name.clone(), value.clone());
            }
        }
    }
    out
}

/// Seed one variable's initial value: the declared default when set,
/// else empty. Kept separate so the admin preview and the detail reader
/// seed the same way.
pub fn seed_value(v: &KbTemplateVariable) -> String {
    v.default.clone().unwrap_or_default()
}

/// Seed every variable from its default.
pub fn seed_values(declared: &[KbTemplateVariable]) -> BTreeMap<String, String> {
    declared
        .iter()
        .map(|v| (v.name.clone(), seed_value(v)))
        .collect()
}

/// Whether a variable renders as a dropdown rather than a free-text input.
pub fn is_allowed_values_picker(v: &KbTemplateVariable) -> bool {
    !v.allowed_values.is_empty()
}

/// Shared reader-side panel: lists an article's declared variables and
/// surfaces whatever error the caller recorded from the last render.
///
/// The caller owns the picks + rendered body + error signals, so the
/// detail page can thread the resulting content back into its Markdown
/// renderer and the admin editor can preview without a round trip.
#[derive(Props, Clone, PartialEq)]
pub struct TemplateVariablesPanelProps {
    /// The article's declared variables, in the order they were authored.
    pub variables: Vec<KbTemplateVariable>,
    /// The reader's current picks, keyed by variable name.
    pub picks: Signal<BTreeMap<String, String>>,
    /// A one-line error above the panel (empty hides it). The caller
    /// fills this from the server's 422 message so the picker row stays
    /// editable while the reader sees what went wrong.
    #[props(default)]
    pub error: String,
    /// Whether a render is in flight; drives the "Updating…" hint under
    /// the heading.
    #[props(default = false)]
    pub busy: bool,
}

#[component]
pub fn TemplateVariablesPanel(props: TemplateVariablesPanelProps) -> Element {
    let variables = props.variables.clone();
    if variables.is_empty() {
        return rsx! {};
    }
    let mut picks = props.picks;
    rsx! {
        div { class: "space-y-3",
            if !props.error.is_empty() {
                p {
                    class: "text-sm leading-5 text-red-600 dark:text-red-400",
                    role: "alert",
                    "{props.error}"
                }
            }
            if props.busy {
                p { class: "text-xs text-subtle", role: "status", "Updating\u{2026}" }
            }
            for v in variables.iter().cloned() {
                {
                    let name = v.name.clone();
                    let label = if v.label.trim().is_empty() {
                        v.name.clone()
                    } else {
                        v.label.clone()
                    };
                    let current = picks.read().get(&v.name).cloned().unwrap_or_default();
                    let field_id = format!("kb-tplvar-{}", v.name);
                    if is_allowed_values_picker(&v) {
                        let options = v.allowed_values.clone();
                        let name_for_change = name.clone();
                        rsx! {
                            div { key: "{v.name}", class: "space-y-1",
                                label {
                                    r#for: "{field_id}",
                                    class: "block text-xs font-medium text-content",
                                    "{label}"
                                }
                                select {
                                    id: "{field_id}",
                                    name: "{v.name}",
                                    class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                                    value: "{current}",
                                    onchange: move |e: FormEvent| {
                                        picks.write().insert(name_for_change.clone(), e.value());
                                    },
                                    if current.is_empty() {
                                        option { value: "", "Choose\u{2026}" }
                                    }
                                    for opt in options.iter().cloned() {
                                        option { value: "{opt}", "{opt}" }
                                    }
                                }
                            }
                        }
                    } else {
                        let name_for_input = name.clone();
                        rsx! {
                            div { key: "{v.name}", class: "space-y-1",
                                label {
                                    r#for: "{field_id}",
                                    class: "block text-xs font-medium text-content",
                                    "{label}"
                                }
                                input {
                                    id: "{field_id}",
                                    name: "{v.name}",
                                    r#type: "text",
                                    class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                                    value: "{current}",
                                    oninput: move |e: FormEvent| {
                                        picks.write().insert(name_for_input.clone(), e.value());
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod kb_template_vars_tests {
    use super::*;

    fn var(name: &str, label: &str, allowed: &[&str], default: Option<&str>) -> KbTemplateVariable {
        KbTemplateVariable {
            name: name.to_string(),
            label: label.to_string(),
            allowed_values: allowed.iter().map(|s| s.to_string()).collect(),
            default: default.map(|s| s.to_string()),
        }
    }

    /// The request body matches the server's `RenderKbArticleRequest`
    /// shape: `{"values": {name: value}}`.
    #[test]
    fn render_values_serializes_as_the_server_expects() {
        let declared = vec![
            var(
                "environment",
                "Environment",
                &["prod", "staging"],
                Some("prod"),
            ),
            var("hostname", "Host", &[], None),
        ];
        let mut picks = BTreeMap::new();
        picks.insert("environment".to_string(), "staging".to_string());
        picks.insert("hostname".to_string(), "db-01".to_string());
        let values = render_values(&declared, &picks);

        let req = crate::modules::kb::RenderKbArticleRequest { values };
        let json = serde_json::to_value(&req).expect("serializes");
        assert_eq!(
            json,
            serde_json::json!({
                "values": {
                    "environment": "staging",
                    "hostname": "db-01",
                }
            })
        );
    }

    /// An empty value is treated as unset: the server renders the
    /// `[missing: NAME]` placeholder, which is the agreed behaviour for a
    /// variable with no default and no allowed values.
    #[test]
    fn empty_values_are_dropped_from_the_request() {
        let declared = vec![var("hostname", "Host", &[], None)];
        let mut picks = BTreeMap::new();
        picks.insert("hostname".to_string(), String::new());
        let values = render_values(&declared, &picks);
        assert!(values.is_empty(), "{values:?}");
    }

    /// A pick for a variable the article no longer declares is dropped,
    /// so a stale local signal cannot trip the server's 400 "unknown
    /// name" check.
    #[test]
    fn unknown_names_are_dropped() {
        let declared = vec![var("env", "Env", &["a", "b"], Some("a"))];
        let mut picks = BTreeMap::new();
        picks.insert("env".to_string(), "a".to_string());
        picks.insert("ghost".to_string(), "x".to_string());
        let values = render_values(&declared, &picks);
        assert_eq!(values.len(), 1);
        assert_eq!(values.get("env").map(String::as_str), Some("a"));
    }

    /// A variable with `allowed_values` is a picker; without, free text.
    #[test]
    fn allowed_values_picks_the_select_shape() {
        let picker = var("env", "Env", &["a", "b"], Some("a"));
        let free = var("hostname", "Host", &[], None);
        assert!(is_allowed_values_picker(&picker));
        assert!(!is_allowed_values_picker(&free));
    }

    /// The initial seed uses each variable's `default`; an unset default
    /// seeds as empty so the server's `[missing: NAME]` placeholder is
    /// what the reader sees until they pick.
    #[test]
    fn seed_values_fills_from_defaults_only() {
        let declared = vec![
            var("env", "Env", &["a", "b"], Some("a")),
            var("hostname", "Host", &[], None),
        ];
        let seeded = seed_values(&declared);
        assert_eq!(seeded.get("env").map(String::as_str), Some("a"));
        assert_eq!(seeded.get("hostname").map(String::as_str), Some(""));
    }

    /// The rendered panel declares a `<select>` for a picker variable and
    /// an `<input>` for a free-text variable. Confirmed structurally
    /// against the source so the test is immune to Dioxus render
    /// internals, mirroring the `collapsible_card` test shape.
    #[test]
    fn the_panel_renders_a_select_for_a_picker_and_an_input_for_free_text() {
        const SRC: &str = include_str!("kb_template_vars.rs");
        let end = SRC
            .find("mod kb_template_vars_tests")
            .expect("this module is part of this file");
        let code = &SRC[..end];
        assert!(
            code.contains("select {"),
            "panel renders a <select> for a picker variable"
        );
        assert!(
            code.contains("input {"),
            "panel renders an <input> for a free-text variable"
        );
        assert!(
            code.contains("is_allowed_values_picker(&v)"),
            "panel branches on allowed_values to pick the shape"
        );
    }
}

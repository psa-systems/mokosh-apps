//! Form input components

use dioxus::prelude::*;

/// Text input component props
#[derive(Props, Clone, PartialEq)]
pub struct InputProps {
    /// Input name attribute
    name: String,
    /// Input label
    #[props(default)]
    label: String,
    /// Input type (text, email, password, etc.)
    #[props(default = "text".to_string())]
    r#type: String,
    /// `step` attribute for `type="number"` inputs. Without it the browser
    /// defaults to `step=1` and rejects fractional values (e.g. `2.5`).
    /// No-op for non-number inputs.
    #[props(default)]
    step: Option<String>,
    /// `min` attribute for `type="number"` inputs. Bounds the field below so
    /// the browser rejects negative values. No-op for non-number inputs.
    #[props(default)]
    min: Option<String>,
    /// `max` attribute for `type="number"` inputs. Bounds the field above so
    /// the browser rejects absurd magnitudes. No-op for non-number inputs.
    #[props(default)]
    max: Option<String>,
    /// `maxlength` attribute for text inputs. A client-side UX nicety that
    /// stops the field from exceeding a known server limit (e.g. ticket
    /// Title at 500, contract Name at 200, company fields at 255); the server
    /// stays the source of truth (MAPPS-210 / MAPPS-211 / MAPPS-213).
    #[props(default)]
    maxlength: Option<i64>,
    /// Placeholder text
    #[props(default)]
    placeholder: String,
    /// Current value
    #[props(default)]
    value: String,
    /// Whether input is required
    #[props(default = false)]
    required: bool,
    /// Whether input is disabled
    #[props(default = false)]
    disabled: bool,
    /// Error message. An explicit value here (e.g. a server `field_message`)
    /// always wins over the component's own rule-based validation below.
    #[props(default)]
    error: String,
    /// Validation rules evaluated against `value` once the field is touched
    /// (PMS-516). The first failing rule's message renders in the inline error
    /// slot. Empty by default, so existing call sites are unaffected until they
    /// opt in. `error` overrides these. Submit-time validation (forcing every
    /// field to evaluate even if never focused) lands with the form submit
    /// guard (PMS-517); this component validates on blur and on subsequent input.
    #[props(default)]
    rules: Vec<crate::utils::validation::Rule>,
    /// Help text
    #[props(default)]
    help: String,
    /// Additional CSS classes
    #[props(default)]
    class: String,
    /// Change handler
    #[props(default)]
    oninput: EventHandler<FormEvent>,
    /// Stable selector for browser-automation tests (PMC-111).
    #[props(default)]
    data_testid: Option<String>,
    /// MAPPS-314: optional `aria-label` for visually-unlabeled inputs
    /// (e.g. `GlobalSearch` which renders a placeholder + magnifier
    /// only). Skipped when empty so the existing per-field visible
    /// `<label for>` keeps doing the work for the common case.
    #[props(default)]
    aria_label: String,
}

/// Text input component
#[component]
pub fn Input(props: InputProps) -> Element {
    // PMS-516: component-owned validation. The field is "touched" once the user
    // blurs it; from then on it re-validates on every keystroke so the error
    // clears as the value is corrected. An explicit `error` prop (server field
    // error) always wins over the rule-based message.
    let mut touched = use_signal(|| false);
    let shown_error = if !props.error.is_empty() {
        props.error.clone()
    } else if touched() {
        crate::utils::validation::validate(&props.value, &props.label, &props.rules)
            .unwrap_or_default()
    } else {
        String::new()
    };

    let input_class = if shown_error.is_empty() {
        "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm"
    } else {
        "block w-full rounded-md border-red-300 shadow-sm focus:border-red-500 focus:ring-red-500 bg-surface dark:border-red-600 text-content sm:text-sm"
    };

    let class = format!("{} {}", input_class, props.class);

    rsx! {
        div { class: "space-y-1",
            if !props.label.is_empty() {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-content",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 ml-1", aria_label: "required", role: "img", "*" }
                    }
                }
            }
            // MAPPS-277: stop attaching the HTML5 `required` attribute, which
            // surfaces the browser-native "Please fill out this field" tooltip
            // on submit. Forms now route every required-field check through
            // their own per-field validators (e.g. `validate_name_field`,
            // MAPPS-281 trim-and-set inline error), so the cue surfaces as a
            // styled inline error message under the field instead of the OS
            // bubble. Keep `aria-required` so assistive tech still announces
            // the field as required, and keep the visible asterisk in the
            // label for sighted users.
            input {
                id: "{props.name}",
                name: "{props.name}",
                r#type: "{props.r#type}",
                step: props.step.as_deref(),
                min: props.min.as_deref(),
                max: props.max.as_deref(),
                maxlength: props.maxlength,
                class: "{class}",
                placeholder: "{props.placeholder}",
                value: "{props.value}",
                aria_required: if props.required { "true" } else { "false" },
                aria_label: if props.aria_label.is_empty() { None } else { Some(props.aria_label.clone()) },
                disabled: props.disabled,
                "data-testid": props.data_testid.as_deref(),
                oninput: move |e| props.oninput.call(e),
                onblur: move |_| touched.set(true),
            }
            if !shown_error.is_empty() {
                p { class: "text-sm leading-5 text-red-600 dark:text-red-400",
                    "{shown_error}"
                }
            } else if !props.help.is_empty() {
                p { class: "text-sm leading-5 text-muted",
                    "{props.help}"
                }
            }
        }
    }
}

/// Shared date-field props (MAPPS-204). The single date-input pattern for the
/// whole app: a native `<input type="date">` (calendar picker) with consistent
/// `min`/`max` bounds so an out-of-range date is rejected by the picker rather
/// than silently saved.
///
/// Behavior (documented, consistent everywhere):
/// - The native picker only ever yields a complete date or an empty string, so
///   a half-entered date can't be saved.
/// - `required: true` marks the field with an asterisk + `aria_required` (the
///   native HTML `required` attribute is NOT set - MAPPS-277 removed it because
///   every form `prevent_default`s, so it would never fire). Enforcement comes
///   from passing `rules` (PMS-516, e.g. `[Rule::Required]`), which the
///   underlying [`Input`] validates on blur and drives into the inline error
///   slot; otherwise the form's submit handler is responsible for rejecting an
///   empty value.
/// - An empty optional field saves as "no date" (the documented default).
#[derive(Props, Clone, PartialEq)]
pub struct DateFieldProps {
    name: String,
    #[props(default)]
    label: String,
    #[props(default)]
    value: String,
    #[props(default = false)]
    required: bool,
    #[props(default = false)]
    disabled: bool,
    #[props(default)]
    error: String,
    /// Validation rules (PMS-516); forwarded to the underlying [`Input`].
    #[props(default)]
    rules: Vec<crate::utils::validation::Rule>,
    #[props(default)]
    help: String,
    /// Earliest selectable date. Defaults to a sane lower bound so a fumbled
    /// year (e.g. `0007`) is rejected by the picker.
    #[props(default = "2000-01-01".to_string())]
    min: String,
    /// Latest selectable date.
    #[props(default = "2100-12-31".to_string())]
    max: String,
    #[props(default)]
    oninput: EventHandler<FormEvent>,
}

/// The single date input used across the app (MAPPS-204). Thin wrapper over
/// [`Input`] that fixes `type="date"` and applies consistent bounds, so every
/// screen's date selection looks and behaves the same.
#[component]
pub fn DateField(props: DateFieldProps) -> Element {
    rsx! {
        Input {
            name: props.name,
            label: props.label,
            r#type: "date",
            min: props.min,
            max: props.max,
            value: props.value,
            required: props.required,
            disabled: props.disabled,
            error: props.error,
            rules: props.rules,
            help: props.help,
            oninput: move |e| props.oninput.call(e),
        }
    }
}

/// Textarea component props
#[derive(Props, Clone, PartialEq)]
pub struct TextareaProps {
    name: String,
    #[props(default)]
    label: String,
    #[props(default)]
    placeholder: String,
    #[props(default)]
    value: String,
    #[props(default = 3)]
    rows: u32,
    #[props(default = false)]
    required: bool,
    #[props(default = false)]
    disabled: bool,
    #[props(default)]
    error: String,
    /// Validation rules (PMS-516); see [`InputProps::rules`]. `error` overrides.
    #[props(default)]
    rules: Vec<crate::utils::validation::Rule>,
    #[props(default)]
    help: String,
    /// `maxlength` attribute. Caps how many characters the textarea accepts so
    /// over-long text is blocked at the input rather than failing server-side.
    /// `i64` to match [`Input::maxlength`] so both take a bare integer cap.
    #[props(default)]
    maxlength: Option<i64>,
    #[props(default)]
    class: String,
    #[props(default)]
    oninput: EventHandler<FormEvent>,
}

#[component]
pub fn Textarea(props: TextareaProps) -> Element {
    // PMS-516: component-owned validation (see `Input`). `error` overrides.
    let mut touched = use_signal(|| false);
    let shown_error = if !props.error.is_empty() {
        props.error.clone()
    } else if touched() {
        crate::utils::validation::validate(&props.value, &props.label, &props.rules)
            .unwrap_or_default()
    } else {
        String::new()
    };

    let input_class = if shown_error.is_empty() {
        "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm"
    } else {
        "block w-full rounded-md border-red-300 shadow-sm focus:border-red-500 focus:ring-red-500 bg-surface dark:border-red-600 text-content sm:text-sm"
    };

    let class = format!("{} {}", input_class, props.class);

    rsx! {
        div { class: "space-y-1",
            if !props.label.is_empty() {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-content",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 ml-1", aria_label: "required", role: "img", "*" }
                    }
                }
            }
            // MAPPS-277: drop HTML5 `required` so the browser-native tooltip
            // doesn't fire; keep `aria-required` for assistive tech and the
            // asterisk in the label for sighted users. Forms validate
            // required textareas (e.g. ticket description) in their submit
            // handler and surface the error inline via `props.error`.
            textarea {
                id: "{props.name}",
                name: "{props.name}",
                class: "{class}",
                placeholder: "{props.placeholder}",
                rows: "{props.rows}",
                maxlength: props.maxlength,
                aria_required: if props.required { "true" } else { "false" },
                disabled: props.disabled,
                oninput: move |e| props.oninput.call(e),
                onblur: move |_| touched.set(true),
                "{props.value}"
            }
            if !shown_error.is_empty() {
                p { class: "text-sm leading-5 text-red-600 dark:text-red-400",
                    "{shown_error}"
                }
            } else if !props.help.is_empty() {
                p { class: "text-sm leading-5 text-muted",
                    "{props.help}"
                }
            }
        }
    }
}

/// Select option
#[derive(Clone, PartialEq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }
}

/// Select component props
#[derive(Props, Clone, PartialEq)]
pub struct SelectProps {
    name: String,
    #[props(default)]
    label: String,
    options: Vec<SelectOption>,
    #[props(default)]
    value: String,
    #[props(default)]
    placeholder: String,
    #[props(default = false)]
    required: bool,
    #[props(default = false)]
    disabled: bool,
    #[props(default)]
    error: String,
    /// Validation rules (PMS-516); see [`InputProps::rules`]. `error` overrides.
    /// Typically `[Rule::Required]` or `[Rule::Uuid]` for a picker `<select>`.
    #[props(default)]
    rules: Vec<crate::utils::validation::Rule>,
    #[props(default)]
    help: String,
    #[props(default)]
    class: String,
    #[props(default)]
    onchange: EventHandler<FormEvent>,
}

#[component]
pub fn Select(props: SelectProps) -> Element {
    // PMS-516: component-owned validation (see `Input`). `error` overrides.
    let mut touched = use_signal(|| false);
    let shown_error = if !props.error.is_empty() {
        props.error.clone()
    } else if touched() {
        crate::utils::validation::validate(&props.value, &props.label, &props.rules)
            .unwrap_or_default()
    } else {
        String::new()
    };

    let input_class = if shown_error.is_empty() {
        "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm"
    } else {
        "block w-full rounded-md border-red-300 shadow-sm focus:border-red-500 focus:ring-red-500 bg-surface dark:border-red-600 text-content sm:text-sm"
    };

    let class = format!("{} {}", input_class, props.class);

    rsx! {
        div { class: "space-y-1",
            if !props.label.is_empty() {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-content",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 ml-1", aria_label: "required", role: "img", "*" }
                    }
                }
            }
            // MAPPS-270: bind the saved value to the `<select>` element
            // itself (not just per-option `selected` attributes) so the
            // displayed choice follows the controlled `props.value` after
            // an external mutation (e.g. the ticket detail inline editors
            // restart the ticket resource, which re-renders this Select
            // with a new prop value). Without the element-level `value`
            // binding the browser keeps the user's last click on screen
            // even when the underlying state diverges, which read as
            // "the change failed" on the inline Status / Priority /
            // Assigned-To editors (the value did persist, the dropdown
            // just refused to repaint until a manual reload). The
            // per-option `selected` binding stays for the initial paint
            // before Dioxus mounts.
            select {
                id: "{props.name}",
                name: "{props.name}",
                class: "{class}",
                // MAPPS-277: aria-required only; the HTML5 `required` attr
                // would surface the browser-native tooltip on submit, which
                // the form's own validation already replaces inline.
                aria_required: if props.required { "true" } else { "false" },
                disabled: props.disabled,
                value: "{props.value}",
                onchange: move |e| props.onchange.call(e),
                onblur: move |_| touched.set(true),
                if !props.placeholder.is_empty() {
                    option { value: "", disabled: true, selected: props.value.is_empty(),
                        "{props.placeholder}"
                    }
                }
                for opt in props.options.iter() {
                    option {
                        key: "{opt.value}",
                        value: "{opt.value}",
                        disabled: opt.disabled,
                        selected: props.value == opt.value,
                        "{opt.label}"
                    }
                }
            }
            if !shown_error.is_empty() {
                p { class: "text-sm leading-5 text-red-600 dark:text-red-400",
                    "{shown_error}"
                }
            } else if !props.help.is_empty() {
                p { class: "text-sm leading-5 text-muted",
                    "{props.help}"
                }
            }
        }
    }
}

/// Checkbox component props
#[derive(Props, Clone, PartialEq)]
pub struct CheckboxProps {
    name: String,
    label: String,
    #[props(default = false)]
    checked: bool,
    #[props(default = false)]
    disabled: bool,
    #[props(default)]
    error: String,
    #[props(default)]
    help: String,
    #[props(default)]
    class: String,
    #[props(default)]
    onchange: EventHandler<FormEvent>,
}

#[component]
pub fn Checkbox(props: CheckboxProps) -> Element {
    // `bg-surface` styles the unchecked box to match the theme. The
    // @tailwindcss/forms base layer paints the :checked state as a white
    // checkmark SVG over a `currentColor` (accent) fill, but the `bg-surface`
    // utility outranks that base rule and keeps the background light, so in
    // light mode the white check renders white-on-white and is invisible
    // (PMS-577). Re-assert the accent fill (and drop the border) on :checked
    // so the checkmark has contrast in both themes.
    let class = format!(
        "h-4 w-4 rounded border-line text-accent focus:ring-accent bg-surface checked:bg-accent checked:border-transparent {}",
        props.class
    );

    rsx! {
        div { class: "flex items-start",
            div { class: "flex items-center h-5",
                input {
                    id: "{props.name}",
                    name: "{props.name}",
                    r#type: "checkbox",
                    class: "{class}",
                    checked: props.checked,
                    disabled: props.disabled,
                    onchange: move |e| props.onchange.call(e),
                }
            }
            div { class: "ml-3 text-sm",
                label {
                    r#for: "{props.name}",
                    class: "font-medium text-content",
                    "{props.label}"
                }
                if !props.error.is_empty() {
                    p { class: "mt-1 text-sm leading-5 text-red-600 dark:text-red-400",
                        "{props.error}"
                    }
                } else if !props.help.is_empty() {
                    p { class: "mt-1 text-sm leading-5 text-muted",
                        "{props.help}"
                    }
                }
            }
        }
    }
}

/// Search input with icon
#[derive(Props, Clone, PartialEq)]
pub struct SearchInputProps {
    #[props(default)]
    value: String,
    #[props(default = "Search…".to_string())]
    placeholder: String,
    #[props(default)]
    class: String,
    #[props(default)]
    oninput: EventHandler<FormEvent>,
}

#[component]
pub fn SearchInput(props: SearchInputProps) -> Element {
    let class = format!(
        "block w-full pl-10 pr-3 py-2 border border-line rounded-md leading-5 bg-surface placeholder-subtle focus:outline-none focus:ring-1 focus:ring-accent focus:border-accent sm:text-sm {}",
        props.class
    );

    rsx! {
        div { class: "relative",
            div { class: "absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none",
                svg {
                    class: "h-5 w-5 text-subtle",
                    xmlns: "http://www.w3.org/2000/svg",
                    view_box: "0 0 20 20",
                    fill: "currentColor",
                    path {
                        fill_rule: "evenodd",
                        d: "M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z",
                        clip_rule: "evenodd",
                    }
                }
            }
            input {
                r#type: "search",
                class: "{class}",
                placeholder: "{props.placeholder}",
                value: "{props.value}",
                oninput: move |e| props.oninput.call(e),
            }
        }
    }
}

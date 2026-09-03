//! Form input components

use dioxus::html::{FileData, HasFileData};
use dioxus::prelude::*;
use std::rc::Rc;

/// A [`FormEvent`]'s payload with its `value()` replaced by the sanitized one
/// (MAPPS-582). Everything else (`valid`, `values`, `files`) delegates to the
/// event the browser raised, so the wrapper is invisible to a call site that
/// reads anything other than the value.
struct SanitizedFormData {
    value: String,
    inner: Rc<FormData>,
}

impl HasFormData for SanitizedFormData {
    fn value(&self) -> String {
        self.value.clone()
    }

    fn valid(&self) -> bool {
        self.inner.valid()
    }

    fn values(&self) -> Vec<(String, FormValue)> {
        self.inner.values()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl HasFileData for SanitizedFormData {
    fn files(&self) -> Vec<FileData> {
        self.inner.files()
    }
}

/// Strip invisible characters from a text event's value before it reaches the
/// call site (MAPPS-582). This is the single choke point: every text-entry
/// surface in the app routes through the components in this file, so a value
/// that renders as nothing can never be read out of `e.value()` and stored.
///
/// Returns the original event untouched when there is nothing to strip, which
/// is every keystroke of ordinary text, so the common path allocates nothing
/// and behaves bit-identically to before. `Event::map` is what rebuilds the
/// event, so the new one shares the original's propagation and
/// `prevent_default` state rather than resetting it.
fn sanitized(e: FormEvent) -> FormEvent {
    let raw = e.value();
    if !crate::utils::text::has_invisible(&raw) {
        return e;
    }
    let data = SanitizedFormData {
        value: crate::utils::text::strip_invisible(&raw),
        inner: e.data(),
    };
    e.map(move |_| FormData::new(data))
}

/// Whether an `<input>` of this type gets its value sanitized (MAPPS-582).
///
/// Everything does except a password. A password may legitimately contain any
/// character, and silently rewriting one turns a correct credential into a
/// failed login with no diagnosis anywhere. Every password, secret and API-key
/// field in the app sets `type="password"`, so this one check exempts them all.
fn sanitizes(field_type: &str) -> bool {
    field_type != "password"
}

/// Bind a text field so editing it clears its inline error slot (MAPPS-581).
/// An inline error describes the value that was submitted, so the next
/// keystroke invalidates it; leaving it on screen reads as the app rejecting
/// what the user just typed.
///
/// The clear lives at the call site, not inside [`Input`], because only the
/// parent knows a submit happened and can re-raise the same message when the
/// corrected value still fails.
pub fn clear_on_edit(
    mut value: Signal<String>,
    mut error: Signal<String>,
) -> impl FnMut(FormEvent) + Copy {
    move |e: FormEvent| {
        error.set(String::new());
        value.set(e.value());
    }
}

/// MAPPS-694: Enter commits an inline-create modal.
///
/// Attach to the modal BODY, never to [`Input`] - MAPPS-347 keeps handlers off
/// the shared field, and a handler on the body means Enter commits from any
/// field in the modal rather than only the one it was wired to.
/// `prevent_default` runs first, so the key never reaches the form the modal
/// opened on top of. Opt-in per call site: a modal that does not attach this
/// keeps Enter doing nothing, which is what a destructive `ConfirmDialog`
/// wants.
pub fn submit_on_enter(mut action: impl FnMut() + 'static) -> impl FnMut(KeyboardEvent) + 'static {
    move |e: KeyboardEvent| {
        if e.key() == Key::Enter {
            e.prevent_default();
            action();
        }
    }
}

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
    /// Blur passthrough (MAPPS-480). Called after the component's own
    /// `touched` bookkeeping, so a call site can act on "the user finished
    /// with this field" (the Website field probes the value it was given)
    /// without taking over the validation state. Defaults to a no-op, so a
    /// call site that does not need it is unaffected.
    #[props(default)]
    onblur: EventHandler<FocusEvent>,
    /// Stable selector for browser-automation tests (PMC-111).
    #[props(default)]
    data_testid: Option<String>,
    /// MAPPS-314: optional `aria-label` for visually-unlabeled inputs
    /// (e.g. `GlobalSearch` which renders a placeholder + magnifier
    /// only). Skipped when empty so the existing per-field visible
    /// `<label for>` keeps doing the work for the common case.
    #[props(default)]
    aria_label: String,
    /// MAPPS-694: focus this field as soon as it mounts, for a modal that
    /// opens mid-keystroke with the value already prefilled. Default false, so
    /// every existing call site is unchanged and nothing else in the app gains
    /// a focus grab.
    ///
    /// It carries the field into the DOM twice on purpose. The HTML attribute
    /// names the initial focus target for assistive tech, and browsers ignore
    /// it on an element inserted after load, which is every modal field here;
    /// the `onmounted` focus is what actually moves the caret.
    #[props(default = false)]
    autofocus: bool,
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

    // MAPPS-582: read the field type once, out here, so the `oninput` closure
    // captures a bool rather than the prop.
    let sanitize = sanitizes(&props.r#type);
    // MAPPS-694: same reason - the mount handler captures these, not the props.
    let autofocus = props.autofocus;
    let focus_name = props.name.clone();

    rsx! {
        div { class: "space-y-1",
            if !props.label.is_empty() {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-content",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 dark:text-red-400 ml-1", aria_label: "required", role: "img", "*" }
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
                autofocus: autofocus,
                "data-testid": props.data_testid.as_deref(),
                // MAPPS-694: the focus a dynamically-inserted field actually
                // gets. Same mechanism the modal panel uses to take focus on
                // mount (`modal.rs`), so the field wins the race by mounting
                // after the panel that would otherwise keep it.
                onmounted: move |e: MountedEvent| {
                    if !autofocus {
                        return;
                    }
                    let name = focus_name.clone();
                    spawn(async move {
                        if let Err(err) = e.set_focus(true).await {
                            tracing::warn!("could not focus {name}: {err}");
                        }
                    });
                },
                oninput: move |e: FormEvent| {
                    props.oninput.call(if sanitize { sanitized(e) } else { e })
                },
                onblur: move |e| {
                    touched.set(true);
                    props.onblur.call(e);
                },
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
    /// MAPPS-592: keep the label out of the flow while still using it.
    ///
    /// A Markdown field is a toolbar sitting directly on top of the box, and
    /// the label belongs above BOTH. Rendering it here puts it between the two,
    /// which reads as a caption on the toolbar rather than a label on the
    /// field. The host renders it instead; the value is still what names the
    /// field in a validation message and to a screen reader, so this hides it
    /// rather than dropping it.
    #[props(default = false)]
    label_hidden: bool,
    /// `maxlength` attribute. Caps how many characters the textarea accepts so
    /// over-long text is blocked at the input rather than failing server-side.
    /// `i64` to match [`Input::maxlength`] so both take a bare integer cap.
    #[props(default)]
    maxlength: Option<i64>,
    #[props(default)]
    class: String,
    /// PMS-939: classes on the wrapper `div` around label, field and error.
    ///
    /// A field that has to fill a flex column cannot do it from the `<textarea>`
    /// alone: the wrapper is the flex item, so a `flex-1` on the field inside it
    /// stretches nothing. Empty for every host that just wants `rows`.
    #[props(default)]
    wrapper_class: String,
    #[props(default)]
    oninput: EventHandler<FormEvent>,
    /// MAPPS-579: keyboard shortcuts. The editor toolbar needs Cmd/Ctrl+B, I
    /// and K on the body field, and the handler has to sit on the textarea
    /// itself so a shortcut only fires while the author is typing in it.
    #[props(default)]
    onkeydown: EventHandler<KeyboardEvent>,
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
    let wrapper = format!("space-y-1 {}", props.wrapper_class);

    rsx! {
        div { class: "{wrapper}",
            if !props.label.is_empty() && !props.label_hidden {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-content",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 dark:text-red-400 ml-1", aria_label: "required", role: "img", "*" }
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
                // MAPPS-582: same choke point as `Input`. A textarea has no
                // password variant, so there is nothing to exempt.
                // MAPPS-585: the value is an ATTRIBUTE, never a child.
                //
                // A `<textarea>`'s text child is its DEFAULT value: the browser
                // copies it into `.value` only while the element is still
                // "clean". The first keystroke dirties it, and from then on
                // writing the child changes `textContent` and nothing the
                // author can see. That is why every toolbar action stopped
                // working the moment anyone typed - the source signal and the
                // preview updated, the field did not, and the next keystroke
                // sent the stale DOM text back up and overwrote the transform.
                //
                // `Input` has always passed `value:` here. This is the same
                // choke point, and dioxus maps the `value` attribute onto the
                // `.value` PROPERTY, which is what a dirty element reads.
                value: "{props.value}",
                oninput: move |e: FormEvent| props.oninput.call(sanitized(e)),
                onkeydown: move |e| props.onkeydown.call(e),
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
                        span { class: "text-red-500 dark:text-red-400 ml-1", aria_label: "required", role: "img", "*" }
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

/// File input props (MAPPS-440). The one input type `form.rs` had no component
/// for, so its 130-character `file:` class recipe sat copied verbatim across the
/// onboarding logo field, the settings logo field and the JSON import picker,
/// with nothing keeping the three in step.
#[derive(Props, Clone, PartialEq)]
pub struct FileFieldProps {
    /// Used for both `id` and `name`, so the label's `for` associates.
    name: String,
    #[props(default)]
    label: String,
    /// `accept` attribute (e.g. `image/png,image/jpeg`). Omitted when empty.
    #[props(default)]
    accept: String,
    #[props(default = false)]
    disabled: bool,
    /// Help text, shown when `error` is empty.
    #[props(default)]
    help: String,
    /// Error message. Wins over `help` and carries `role="alert"`.
    #[props(default)]
    error: String,
    /// Rendered in the help slot ahead of `error` and `help`, for the state an
    /// upload-on-selection field is in while the request is in flight.
    #[props(default)]
    status: String,
    /// Optional content between the label and the input (e.g. the current
    /// logo and its Remove button).
    preview: Option<Element>,
    #[props(default)]
    onchange: EventHandler<FormEvent>,
}

/// The single file input used across the app (MAPPS-440). Mirrors [`Input`]'s
/// label + help + error structure and owns the `file:` class recipe.
#[component]
pub fn FileField(props: FileFieldProps) -> Element {
    rsx! {
        div { class: "space-y-1",
            if !props.label.is_empty() {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-content",
                    "{props.label}"
                }
            }
            if let Some(preview) = props.preview {
                {preview}
            }
            input {
                id: "{props.name}",
                name: "{props.name}",
                r#type: "file",
                accept: if props.accept.is_empty() { None } else { Some(props.accept.clone()) },
                disabled: props.disabled,
                class: "block w-full text-sm text-content file:mr-3 file:rounded-md file:border-0 file:bg-surface-2 file:px-3 file:py-1.5 file:text-sm file:font-medium",
                onchange: move |e| props.onchange.call(e),
            }
            if !props.status.is_empty() {
                p { class: "text-xs text-muted", "{props.status}" }
            } else if !props.error.is_empty() {
                p { class: "text-xs text-red-600 dark:text-red-400", role: "alert", "{props.error}" }
            } else if !props.help.is_empty() {
                p { class: "text-xs text-muted", "{props.help}" }
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
                // MAPPS-582: a query carrying an invisible character matches
                // nothing and reads as "the record is gone".
                oninput: move |e: FormEvent| props.oninput.call(sanitized(e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::events::HasKeyboardData;
    use dioxus::prelude::{Code, Location, Modifiers, ModifiersInteraction};
    use std::cell::Cell;

    /// The shape a browser hands `oninput`: a value plus the form's named
    /// values. Enough to drive [`sanitized`] end to end.
    struct StubFormData {
        value: String,
    }

    impl HasFormData for StubFormData {
        fn value(&self) -> String {
            self.value.clone()
        }

        fn valid(&self) -> bool {
            true
        }

        fn values(&self) -> Vec<(String, FormValue)> {
            vec![("name".to_string(), FormValue::Text(self.value.clone()))]
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    impl HasFileData for StubFormData {
        fn files(&self) -> Vec<FileData> {
            Vec::new()
        }
    }

    /// This file up to the first test module, so a needle below cannot match
    /// itself.
    fn component_source() -> &'static str {
        include_str!("form.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap()
    }

    fn event(value: &str) -> FormEvent {
        FormEvent::new(
            Rc::new(FormData::new(StubFormData {
                value: value.to_string(),
            })),
            true,
        )
    }

    #[test]
    fn sanitized_strips_the_invisible_characters() {
        assert_eq!(
            sanitized(event("919-397-4144\u{200B}")).value(),
            "919-397-4144"
        );
        assert_eq!(sanitized(event("Acme\u{FEFF}")).value(), "Acme");
        assert_eq!(sanitized(event("John\u{00A0}Smith")).value(), "John Smith");
    }

    /// The common path: nothing to strip, so the original event is handed on
    /// untouched rather than rebuilt.
    #[test]
    fn a_clean_value_takes_the_unchanged_path() {
        let e = event("John Smith");
        let original = e.data();
        let out = sanitized(e);
        assert_eq!(out.value(), "John Smith");
        assert!(
            Rc::ptr_eq(&original, &out.data()),
            "a clean value must not be rebuilt"
        );
    }

    /// Typing is unaffected: a mid-value space is kept where it was typed and
    /// a trailing space while typing is not eaten.
    #[test]
    fn typing_is_unaffected() {
        for value in ["John ", " John", "John  Smith", "line one\nline two"] {
            let e = event(value);
            let original = e.data();
            let out = sanitized(e);
            assert_eq!(out.value(), value);
            assert!(Rc::ptr_eq(&original, &out.data()), "{value:?} was rebuilt");
        }
    }

    /// The wrapper only replaces `value`; everything else still comes from the
    /// event the browser raised.
    #[test]
    fn the_wrapper_delegates_everything_but_the_value() {
        let out = sanitized(event("Acme\u{200B}"));
        assert!(out.valid());
        assert_eq!(
            out.values(),
            vec![(
                "name".to_string(),
                FormValue::Text("Acme\u{200B}".to_string())
            )]
        );
        assert!(out.files().is_empty());
    }

    /// MAPPS-582: a password may legitimately contain any character, and
    /// silently rewriting one turns a correct credential into a failed login
    /// with no diagnosis. `Input` gates the sanitizing on the field type, which
    /// is what exempts every password / secret / API-key field at once.
    #[test]
    fn password_inputs_are_exempt() {
        assert!(!sanitizes("password"), "a password is never rewritten");
        for kind in ["text", "email", "tel", "url", "search", "number", "date"] {
            assert!(sanitizes(kind), "{kind} must be sanitized");
        }

        // What `Input` does with that answer, applied to a password whose value
        // is nothing but characters this would otherwise strip.
        let raw = "hunter2\u{200B}\u{00A0}\u{200D}\u{FEFF}";
        let e = event(raw);
        let out = if sanitizes("password") {
            sanitized(e)
        } else {
            e
        };
        assert_eq!(
            out.value(),
            raw,
            "a password must pass through byte-identical"
        );

        let src = component_source();
        assert!(
            src.contains("let sanitize = sanitizes(&props.r#type);"),
            "Input must gate its sanitizing on the field type"
        );
        assert!(
            src.contains("props.oninput.call(if sanitize { sanitized(e) } else { e })"),
            "and must pass a password value through untouched"
        );
    }

    /// The choke point only works if every text-entry element in this file
    /// goes through it. `Textarea` and `SearchInput` have no password variant,
    /// so they sanitize unconditionally.
    #[test]
    fn every_text_element_routes_through_the_choke_point() {
        let src = component_source();
        assert_eq!(
            src.matches("props.oninput.call(sanitized(e))").count(),
            2,
            "Textarea and SearchInput must both sanitize"
        );
    }

    // -- MAPPS-694: the inline-create modal keyboard path ------------------

    /// A keydown as the browser delivers one, so [`submit_on_enter`] is driven
    /// rather than read.
    struct StubKey(Key);

    impl ModifiersInteraction for StubKey {
        fn modifiers(&self) -> Modifiers {
            Modifiers::empty()
        }
    }

    impl HasKeyboardData for StubKey {
        fn key(&self) -> Key {
            self.0.clone()
        }

        fn code(&self) -> Code {
            Code::Unidentified
        }

        fn location(&self) -> Location {
            Location::Standard
        }

        fn is_auto_repeating(&self) -> bool {
            false
        }

        fn is_composing(&self) -> bool {
            false
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    fn keydown(key: Key) -> KeyboardEvent {
        KeyboardEvent::new(Rc::new(KeyboardData::new(StubKey(key))), true)
    }

    /// Counts the create action, so a handler can be checked for having run it
    /// exactly once.
    fn counting_handler() -> (Rc<Cell<usize>>, impl FnMut(KeyboardEvent)) {
        let runs = Rc::new(Cell::new(0usize));
        let counter = runs.clone();
        (
            runs,
            submit_on_enter(move || counter.set(counter.get() + 1)),
        )
    }

    /// MAPPS-694: Enter in an inline-create modal runs the create action, and
    /// the key stops there. Without the `prevent_default` the same Enter is
    /// also the implicit submit of the form the modal opened on top of, so one
    /// keystroke would create the company AND submit the half-filled ticket
    /// behind it.
    #[test]
    fn enter_runs_the_action_and_never_reaches_the_form_behind() {
        let (runs, mut handler) = counting_handler();
        let e = keydown(Key::Enter);
        handler(e.clone());

        assert_eq!(runs.get(), 1, "Enter runs the create action");
        assert!(
            !e.default_action_enabled(),
            "and is consumed, so it does not submit the form behind the modal"
        );
    }

    /// Every other key is left exactly as it was. Escape matters most: the
    /// modal's own cancel is `ModalChrome`'s `onkeydown`, and it only still
    /// works because this handler neither runs the create nor consumes the key.
    #[test]
    fn every_other_key_is_left_alone() {
        for key in [
            Key::Escape,
            Key::Tab,
            Key::ArrowDown,
            Key::Character(" ".to_string()),
            Key::Character("a".to_string()),
        ] {
            let (runs, mut handler) = counting_handler();
            let e = keydown(key.clone());
            handler(e.clone());

            assert_eq!(runs.get(), 0, "{key:?} must not create anything");
            assert!(
                e.default_action_enabled(),
                "{key:?} must keep its default action"
            );
        }
    }

    fn render(app: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    #[component]
    fn FocusedField() -> Element {
        rsx! {
            Input { name: "new_company_name", label: "Company name", autofocus: true }
        }
    }

    #[component]
    fn PlainField() -> Element {
        rsx! {
            Input { name: "company_search", label: "Company" }
        }
    }

    /// MAPPS-694: the field a modal opens on says so in the markup, and every
    /// other field in the app is unchanged because the prop defaults off.
    #[test]
    fn autofocus_marks_only_the_field_that_asked_for_it() {
        let focused = render(FocusedField);
        assert!(
            focused.contains("autofocus"),
            "an autofocus field names itself as the initial focus target; got: {focused}"
        );

        let plain = render(PlainField);
        assert!(
            !plain.contains("autofocus"),
            "and the default is off, so no other field grabs focus; got: {plain}"
        );
    }

    /// The attribute alone does nothing here: a browser ignores `autofocus` on
    /// an element inserted after load, which is every field in a modal. The
    /// focus that actually moves the caret happens on mount.
    #[test]
    fn autofocus_moves_the_caret_on_mount() {
        let src = component_source();
        assert!(
            src.contains("e.set_focus(true).await"),
            "Input must focus the field when it mounts, not only mark it"
        );
    }
}

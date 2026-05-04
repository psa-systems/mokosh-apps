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
    /// Error message
    #[props(default)]
    error: String,
    /// Help text
    #[props(default)]
    help: String,
    /// Additional CSS classes
    #[props(default)]
    class: String,
    /// Change handler
    #[props(default)]
    oninput: EventHandler<FormEvent>,
}

/// Text input component
#[component]
pub fn Input(props: InputProps) -> Element {
    let input_class = if props.error.is_empty() {
        "block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:text-white sm:text-sm"
    } else {
        "block w-full rounded-md border-red-300 shadow-sm focus:border-red-500 focus:ring-red-500 dark:bg-gray-700 dark:border-red-600 dark:text-white sm:text-sm"
    };

    let class = format!("{} {}", input_class, props.class);

    rsx! {
        div { class: "space-y-1",
            if !props.label.is_empty() {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 ml-1", "*" }
                    }
                }
            }
            input {
                id: "{props.name}",
                name: "{props.name}",
                r#type: "{props.r#type}",
                class: "{class}",
                placeholder: "{props.placeholder}",
                value: "{props.value}",
                required: props.required,
                disabled: props.disabled,
                oninput: move |e| props.oninput.call(e),
            }
            if !props.error.is_empty() {
                p { class: "text-sm text-red-600 dark:text-red-400",
                    "{props.error}"
                }
            } else if !props.help.is_empty() {
                p { class: "text-sm text-gray-500 dark:text-gray-400",
                    "{props.help}"
                }
            }
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
    #[props(default)]
    help: String,
    #[props(default)]
    class: String,
    #[props(default)]
    oninput: EventHandler<FormEvent>,
}

#[component]
pub fn Textarea(props: TextareaProps) -> Element {
    let input_class = if props.error.is_empty() {
        "block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:text-white sm:text-sm"
    } else {
        "block w-full rounded-md border-red-300 shadow-sm focus:border-red-500 focus:ring-red-500 dark:bg-gray-700 dark:border-red-600 dark:text-white sm:text-sm"
    };

    let class = format!("{} {}", input_class, props.class);

    rsx! {
        div { class: "space-y-1",
            if !props.label.is_empty() {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 ml-1", "*" }
                    }
                }
            }
            textarea {
                id: "{props.name}",
                name: "{props.name}",
                class: "{class}",
                placeholder: "{props.placeholder}",
                rows: "{props.rows}",
                required: props.required,
                disabled: props.disabled,
                oninput: move |e| props.oninput.call(e),
                "{props.value}"
            }
            if !props.error.is_empty() {
                p { class: "text-sm text-red-600 dark:text-red-400",
                    "{props.error}"
                }
            } else if !props.help.is_empty() {
                p { class: "text-sm text-gray-500 dark:text-gray-400",
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
    #[props(default)]
    help: String,
    #[props(default)]
    class: String,
    #[props(default)]
    onchange: EventHandler<FormEvent>,
}

#[component]
pub fn Select(props: SelectProps) -> Element {
    let input_class = if props.error.is_empty() {
        "block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:text-white sm:text-sm"
    } else {
        "block w-full rounded-md border-red-300 shadow-sm focus:border-red-500 focus:ring-red-500 dark:bg-gray-700 dark:border-red-600 dark:text-white sm:text-sm"
    };

    let class = format!("{} {}", input_class, props.class);

    rsx! {
        div { class: "space-y-1",
            if !props.label.is_empty() {
                label {
                    r#for: "{props.name}",
                    class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                    "{props.label}"
                    if props.required {
                        span { class: "text-red-500 ml-1", "*" }
                    }
                }
            }
            select {
                id: "{props.name}",
                name: "{props.name}",
                class: "{class}",
                required: props.required,
                disabled: props.disabled,
                onchange: move |e| props.onchange.call(e),
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
            if !props.error.is_empty() {
                p { class: "text-sm text-red-600 dark:text-red-400",
                    "{props.error}"
                }
            } else if !props.help.is_empty() {
                p { class: "text-sm text-gray-500 dark:text-gray-400",
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
    help: String,
    #[props(default)]
    class: String,
    #[props(default)]
    onchange: EventHandler<FormEvent>,
}

#[component]
pub fn Checkbox(props: CheckboxProps) -> Element {
    let class = format!(
        "h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-700 {}",
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
                    class: "font-medium text-gray-700 dark:text-gray-300",
                    "{props.label}"
                }
                if !props.help.is_empty() {
                    p { class: "text-gray-500 dark:text-gray-400",
                        "{props.help}"
                    }
                }
            }
        }
    }
}

/// Form field wrapper with consistent spacing
#[derive(Props, Clone, PartialEq)]
pub struct FormFieldProps {
    children: Element,
    #[props(default)]
    class: String,
}

#[component]
pub fn FormField(props: FormFieldProps) -> Element {
    let class = format!("space-y-1 {}", props.class);

    rsx! {
        div { class: "{class}",
            {props.children}
        }
    }
}

/// Form section with title and description
#[derive(Props, Clone, PartialEq)]
pub struct FormSectionProps {
    children: Element,
    title: String,
    #[props(default)]
    description: String,
    #[props(default)]
    class: String,
}

#[component]
pub fn FormSection(props: FormSectionProps) -> Element {
    let class = format!("space-y-6 {}", props.class);

    rsx! {
        div { class: "{class}",
            div {
                h3 { class: "text-lg font-medium text-gray-900 dark:text-white",
                    "{props.title}"
                }
                if !props.description.is_empty() {
                    p { class: "mt-1 text-sm text-gray-500 dark:text-gray-400",
                        "{props.description}"
                    }
                }
            }
            div { class: "mt-6 grid grid-cols-1 gap-y-6 gap-x-4 sm:grid-cols-6",
                {props.children}
            }
        }
    }
}

/// Search input with icon
#[derive(Props, Clone, PartialEq)]
pub struct SearchInputProps {
    #[props(default)]
    value: String,
    #[props(default = "Search...".to_string())]
    placeholder: String,
    #[props(default)]
    class: String,
    #[props(default)]
    oninput: EventHandler<FormEvent>,
}

#[component]
pub fn SearchInput(props: SearchInputProps) -> Element {
    let class = format!(
        "block w-full pl-10 pr-3 py-2 border border-gray-300 rounded-md leading-5 bg-white dark:bg-gray-700 dark:border-gray-600 placeholder-gray-500 focus:outline-none focus:placeholder-gray-400 focus:ring-1 focus:ring-blue-500 focus:border-blue-500 sm:text-sm {}",
        props.class
    );

    rsx! {
        div { class: "relative",
            div { class: "absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none",
                svg {
                    class: "h-5 w-5 text-gray-400",
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

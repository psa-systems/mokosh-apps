//! Accessible icon-only button primitive (MAPPS-261).
//!
//! Icon-only controls (edit / delete / close / nav toggles) render no visible
//! text, so screen-reader and keyboard users have nothing to read and there is
//! no hover tooltip. `IconButton` enforces both: the `label` prop is REQUIRED
//! (no `#[props(default)]`), and it is emitted as BOTH `aria-label` (accessible
//! name) and `title` (hover/focus tooltip). New icon-only sites should adopt
//! this wrapper so an accessible name can never be forgotten.

use dioxus::prelude::*;

/// Props for [`IconButton`].
#[derive(Props, Clone, PartialEq)]
pub struct IconButtonProps {
    /// The icon to render (e.g. `PencilIcon {}`).
    children: Element,
    /// Accessible name. REQUIRED: emitted as both `aria-label` and `title`,
    /// so the control always has a screen-reader name and a hover tooltip.
    label: String,
    /// Additional CSS classes appended after the base styling.
    #[props(default)]
    class: String,
    /// Whether the button is disabled.
    #[props(default = false)]
    disabled: bool,
    /// Button `type` attribute. Defaults to `button` so it never submits a
    /// surrounding form by accident.
    #[props(default = "button".to_string())]
    r#type: String,
    /// Stable selector for browser-automation tests (PMC-111). When set,
    /// emitted as `data-testid="..."` on the rendered `<button>`.
    #[props(default)]
    data_testid: Option<String>,
    /// Click handler.
    #[props(default)]
    onclick: EventHandler<MouseEvent>,
}

/// Reusable icon-only button that always carries an accessible name + tooltip.
#[component]
pub fn IconButton(props: IconButtonProps) -> Element {
    let base_class = "inline-flex items-center justify-center rounded-md focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 disabled:opacity-50 disabled:cursor-not-allowed";
    let class = format!("{} {}", base_class, props.class);

    rsx! {
        button {
            class: "{class}",
            r#type: "{props.r#type}",
            disabled: props.disabled,
            "aria-label": "{props.label}",
            title: "{props.label}",
            "data-testid": props.data_testid.as_deref(),
            onclick: move |e| props.onclick.call(e),
            {props.children}
        }
    }
}

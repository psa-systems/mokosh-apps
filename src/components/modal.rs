//! Modal dialog components

use dioxus::prelude::*;

use super::button::{Button, ButtonVariant};
use super::error_banner::ErrorBanner;
use super::icon_button::IconButton;
use super::icons::XMarkIcon;

/// Modal size options
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ModalSize {
    Small, // max-w-md
    #[default]
    Medium, // max-w-lg
    Large, // max-w-2xl
    XLarge, // max-w-4xl
    Full,  // max-w-7xl
}

impl ModalSize {
    fn class(&self) -> &'static str {
        match self {
            ModalSize::Small => "max-w-md",
            ModalSize::Medium => "max-w-lg",
            ModalSize::Large => "max-w-2xl",
            ModalSize::XLarge => "max-w-4xl",
            ModalSize::Full => "max-w-7xl",
        }
    }
}

/// Modal dialog props
#[derive(Props, Clone, PartialEq)]
pub struct ModalProps {
    /// Modal content
    children: Element,
    /// Whether modal is open
    open: bool,
    /// Modal title
    title: String,
    /// Modal size
    #[props(default)]
    size: ModalSize,
    /// Close handler
    onclose: EventHandler<()>,
    /// Optional footer actions
    footer: Option<Element>,
    /// PMS-763: optional chrome between the header and the scrolling body,
    /// pinned the way they are. For a tab bar or anything else that has to stay
    /// put while the content moves under it.
    ///
    /// A caller CAN do this itself with a `sticky` element at the top of the
    /// body, and the request-form builder did until this existed. It goes wrong
    /// quietly: a sticky box is constrained by the scrollport inset by the
    /// scroll container's padding, so `top-0` parks it 16px below the top of
    /// the visible area and the content scrolls through the gap. Covering that
    /// takes negative margins, a background and a z-index, all tuned to this
    /// component's `px-4 py-4` from the outside, which stops matching the
    /// moment that padding changes.
    subheader: Option<Element>,
}

/// Modal dialog component
#[component]
pub fn Modal(props: ModalProps) -> Element {
    if !props.open {
        return rsx! {};
    }

    // Render the open dialog as a child component so it mounts only while the
    // modal is open. That lets `ModalDialog` use `use_drop` to restore focus on
    // every close path (Esc, backdrop, X, Cancel, or a parent-driven close),
    // since the child unmounts whenever `open` flips to false.
    rsx! {
        ModalDialog {
            title: props.title.clone(),
            size: props.size,
            onclose: props.onclose,
            footer: props.footer.clone(),
            subheader: props.subheader.clone(),
            {props.children}
        }
    }
}

/// Open-modal panel. Only mounted while the modal is open.
#[component]
fn ModalDialog(
    children: Element,
    title: String,
    size: ModalSize,
    onclose: EventHandler<()>,
    footer: Option<Element>,
    subheader: Option<Element>,
) -> Element {
    // Restore focus to the control that opened the modal. Capture the active
    // element on the first render, before the dialog steals focus on mount, and
    // restore it when this component unmounts (any close path). MAPPS-511: both
    // hosts do this now - the desktop parks the element in its webview under
    // the token rather than reading it back into Rust.
    let trigger = use_hook(crate::platform::dom::capture_focus);
    use_drop(move || trigger.restore());

    rsx! {
        ModalChrome { title, size, onclose, footer, subheader, {children} }
    }
}

/// The panel itself: backdrop, pinned header, optional pinned subheader,
/// scrolling body, pinned footer.
///
/// Split out of [`ModalDialog`] so the layout can be rendered in a host test.
/// `ModalDialog` records the focused element to restore focus on close, which
/// needs a document to record from and a host test has none, so the dialog as a
/// whole is not what the host test renders. The structure is what PMS-763 was
/// about, so the structure is what the tests get to see - the real one, not a
/// copy of it.
#[component]
fn ModalChrome(
    children: Element,
    title: String,
    size: ModalSize,
    onclose: EventHandler<()>,
    footer: Option<Element>,
    subheader: Option<Element>,
) -> Element {
    let size_class = size.class();

    rsx! {
        div { class: "fixed inset-0 z-50 overflow-y-auto",
            // Backdrop. Tailwind v4 removed `bg-opacity-*`; use the
            // slash alpha syntax instead so the page stays visible
            // through a 75% gray dim instead of being fully covered.
            div {
                class: "fixed inset-0 bg-gray-500/75 transition-opacity", // theme-guard-allow: overlay scrim
                onclick: move |_| onclose.call(()),
            }

            // Modal container
            div { class: "flex min-h-full items-end justify-center p-4 text-center sm:items-center sm:p-0",
                div {
                    // Cap the panel at 90vh and lay it out as a flex column so the
                    // header and footer stay pinned (flex-shrink-0) while only the
                    // body scrolls. Keeps large modals usable on small screens with
                    // no double-axis scroll.
                    class: "relative transform flex flex-col max-h-[90vh] overflow-hidden rounded-lg bg-raised text-left shadow-xl transition-all sm:my-8 w-full {size_class}",
                    // PMS-369: Esc cancels. Focus the dialog on mount so the
                    // keydown lands here even before the user clicks anything;
                    // keydown from any focused control inside also bubbles up.
                    tabindex: "-1",
                    onmounted: move |e| {
                        spawn(async move {
                            let _ = e.set_focus(true).await;
                        });
                    },
                    onkeydown: move |e: KeyboardEvent| {
                        if e.key() == Key::Escape {
                            onclose.call(());
                        }
                    },
                    onclick: |e| e.stop_propagation(),

                    // Header (pinned)
                    div { class: "flex-shrink-0 flex items-center justify-between px-4 py-3 border-b border-line",
                        h3 { class: "text-lg font-medium text-content",
                            "{title}"
                        }
                        IconButton {
                            label: "Close dialog",
                            class: "rounded-md text-subtle hover:text-content focus:outline-none focus:ring-2 focus:ring-accent",
                            onclick: move |_| onclose.call(()),
                            XMarkIcon {}
                        }
                    }

                    // PMS-763: chrome between the header and the body, pinned
                    // like both of them. Outside the scrolling region on
                    // purpose: a tab bar that lives inside it has to be made to
                    // hover over its own container, and whatever it fails to
                    // cover is a strip of scrolled content sitting above it.
                    if let Some(subheader) = subheader {
                        div { class: "flex-shrink-0 px-4 border-b border-line",
                            {subheader}
                        }
                    }

                    // Body (scrolls within the capped height)
                    div { class: "flex-1 min-h-0 overflow-y-auto px-4 py-4",
                        {children}
                    }

                    // Footer (optional, pinned)
                    if let Some(footer) = footer {
                        div { class: "flex-shrink-0 px-4 py-3 border-t border-line flex justify-end space-x-3",
                            {footer}
                        }
                    }
                }
            }
        }
    }
}

/// Confirmation dialog props
#[derive(Props, Clone, PartialEq)]
pub struct ConfirmDialogProps {
    /// Whether dialog is open
    open: bool,
    /// Dialog title
    title: String,
    /// Dialog message
    message: String,
    /// Confirm button text
    #[props(default = "Confirm".to_string())]
    confirm_text: String,
    /// Cancel button text
    #[props(default = "Cancel".to_string())]
    cancel_text: String,
    /// Whether this is a destructive action
    #[props(default = false)]
    destructive: bool,
    /// Loading state
    #[props(default = false)]
    loading: bool,
    /// PMS-369: type-to-confirm gate for catastrophic / cascading deletes.
    /// When non-empty, the destructive button stays disabled until the user
    /// types this exact phrase (e.g. the company name). Empty (default) = a
    /// plain one-click confirm, which is enough for non-cascading deletes.
    #[props(default)]
    confirm_phrase: String,
    /// MAPPS-574: why the confirmed action did not happen, rendered inside the
    /// dialog. Empty (default) = nothing to report.
    ///
    /// The dialog is the right home for it rather than a banner on the page
    /// behind: the caller keeps `open` true on failure, so the reason appears
    /// next to the button that produced it and above the phrase the user
    /// already typed, instead of on a page they have just been dropped back
    /// onto with no obvious link to what they did.
    #[props(default)]
    error: String,
    /// MAPPS-577: extra content between the message and the phrase input.
    /// Used to show what the action would affect BEFORE the name is typed,
    /// rather than reporting it as a refusal afterwards.
    #[props(default)]
    body: Option<Element>,
    /// MAPPS-577: the action cannot succeed, so the dialog does not pretend it
    /// can. The phrase gate and the confirm button are both withheld: asking
    /// somebody to type a company name to enable a button that will be refused
    /// is the wasted effort this flag exists to remove.
    #[props(default = false)]
    blocked: bool,
    /// MAPPS-577: an alternative action in the footer, for the case where the
    /// destructive one is refused and there IS something useful to do instead.
    #[props(default)]
    alternative: Option<Element>,
    /// Confirm handler
    onconfirm: EventHandler<()>,
    /// Cancel/close handler
    oncancel: EventHandler<()>,
}

/// PMS-369: does the typed text satisfy the type-to-confirm gate? An empty
/// `required` means no gate (always satisfied). Otherwise the typed text must
/// match after trimming, case-insensitively, so " acme corp " passes for
/// "Acme Corp".
pub fn confirm_phrase_satisfied(typed: &str, required: &str) -> bool {
    let required = required.trim();
    required.is_empty() || typed.trim().eq_ignore_ascii_case(required)
}

/// Confirmation dialog component.
///
/// MAPPS-574: the open dialog is a child component, for the same reason
/// [`Modal`] splits out [`ModalDialog`]. The type-to-confirm phrase is state
/// that must not outlive the dialog it belongs to, and `ConfirmDialog` itself
/// does not unmount when the dialog closes - the caller keeps it mounted and
/// flips `open`. Holding `typed` here meant cancelling and reopening found the
/// phrase still in the box and the destructive button already enabled, so every
/// attempt after the first was a one-click delete. Mounting the state with the
/// dialog re-arms the gate on every open, with no reset to remember.
#[component]
pub fn ConfirmDialog(props: ConfirmDialogProps) -> Element {
    if !props.open {
        return rsx! {};
    }

    rsx! {
        OpenConfirmDialog {
            title: props.title.clone(),
            message: props.message.clone(),
            confirm_text: props.confirm_text.clone(),
            cancel_text: props.cancel_text.clone(),
            destructive: props.destructive,
            loading: props.loading,
            confirm_phrase: props.confirm_phrase.clone(),
            error: props.error.clone(),
            body: props.body.clone(),
            blocked: props.blocked,
            alternative: props.alternative.clone(),
            onconfirm: props.onconfirm,
            oncancel: props.oncancel,
        }
    }
}

/// The dialog itself. Only mounted while open, so its phrase state is born and
/// dies with one confirmation attempt.
#[component]
#[allow(clippy::too_many_arguments)]
fn OpenConfirmDialog(
    title: String,
    message: String,
    confirm_text: String,
    cancel_text: String,
    destructive: bool,
    loading: bool,
    confirm_phrase: String,
    error: String,
    body: Option<Element>,
    blocked: bool,
    alternative: Option<Element>,
    onconfirm: EventHandler<()>,
    oncancel: EventHandler<()>,
) -> Element {
    let confirm_variant = if destructive {
        ButtonVariant::Danger
    } else {
        ButtonVariant::Primary
    };

    // PMS-369: type-to-confirm. `typed` tracks the gate input; the destructive
    // button is disabled until it matches `confirm_phrase`. A non-matching
    // (or empty) phrase keeps the button disabled, so a different entity's
    // name never carries over an enabled state.
    let mut typed = use_signal(String::new);
    // MAPPS-577: no gate when the action is refused. There is nothing for the
    // phrase to unlock.
    let gated = !blocked && !confirm_phrase.trim().is_empty();
    let satisfied = confirm_phrase_satisfied(&typed.read(), &confirm_phrase);
    let confirm_disabled = loading || !satisfied;
    let phrase = confirm_phrase.clone();

    rsx! {
        Modal {
            open: true,
            title: title.clone(),
            size: ModalSize::Small,
            onclose: move |_| oncancel.call(()),
            footer: rsx! {
                if let Some(alternative) = alternative {
                    {alternative}
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    onclick: move |_| oncancel.call(()),
                    disabled: loading,
                    "{cancel_text}"
                }
                // MAPPS-577: withheld entirely when the action is refused,
                // rather than rendered disabled. A greyed-out Delete invites
                // the user to work out what would enable it; absent, the
                // alternative beside it is the obvious next move.
                if !blocked {
                    Button {
                        variant: confirm_variant,
                        onclick: move |_| onconfirm.call(()),
                        loading: loading,
                        disabled: confirm_disabled,
                        "{confirm_text}"
                    }
                }
            },
            p { class: "text-sm text-muted",
                "{message}"
            }
            if let Some(body) = body {
                div { class: "mt-3", {body} }
            }
            // MAPPS-574: why the last attempt did not happen. Above the phrase
            // input, so the reason and the control the user is about to reuse
            // are read in that order. `ErrorBanner` carries `role="alert"`, so
            // a refusal arriving into an already-open dialog is announced
            // rather than only appearing.
            if !error.is_empty() {
                div { class: "mt-3",
                    ErrorBanner { "{error}" }
                }
            }
            if gated {
                div { class: "mt-3 space-y-1",
                    label { class: "block text-xs font-medium text-muted",
                        "Type "
                        span { class: "font-semibold text-content", "{phrase}" }
                        " to confirm"
                    }
                    input {
                        r#type: "text",
                        class: "w-full rounded-md border border-line bg-surface px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-red-500",
                        value: "{typed}",
                        autocomplete: "off",
                        // MAPPS-582: raw input. An invisible character pasted
                        // with the phrase would leave the confirm button dead
                        // with the right words on screen.
                        oninput: move |e: FormEvent| {
                            typed.set(crate::utils::text::strip_invisible(&e.value()))
                        },
                    }
                }
            }
        }
    }
}

/// Alert/notification types
#[derive(Clone, Copy, PartialEq, Default)]
pub enum AlertType {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl AlertType {
    fn classes(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            AlertType::Info => (
                "bg-blue-50 dark:bg-blue-900/20",
                "text-blue-400",
                "text-blue-700 dark:text-blue-300",
            ),
            AlertType::Success => (
                "bg-green-50 dark:bg-green-900/20",
                // MAPPS-444: decorative icon, and 400 already clears AA on the
                // dark surface, so it needs no dark pair.
                "text-green-400", // theme-guard-allow
                "text-green-700 dark:text-green-300",
            ),
            AlertType::Warning => (
                "bg-yellow-50 dark:bg-yellow-900/20",
                "text-yellow-400",
                "text-yellow-700 dark:text-yellow-300",
            ),
            AlertType::Error => (
                "bg-red-50 dark:bg-red-900/20",
                // MAPPS-444: decorative icon, and 400 already clears AA on the
                // dark surface, so it needs no dark pair.
                "text-red-400", // theme-guard-allow
                "text-red-700 dark:text-red-300",
            ),
        }
    }
}

/// Alert banner props
#[derive(Props, Clone, PartialEq)]
pub struct AlertProps {
    /// Alert type
    #[props(default)]
    alert_type: AlertType,
    /// Alert title
    #[props(default)]
    title: String,
    /// Alert message
    message: String,
    /// Whether alert can be dismissed
    #[props(default = false)]
    dismissible: bool,
    /// Dismiss handler
    #[props(default)]
    ondismiss: EventHandler<()>,
}

/// Alert banner component
#[component]
pub fn Alert(props: AlertProps) -> Element {
    let (bg_class, icon_class, text_class) = props.alert_type.classes();

    rsx! {
        div { class: "rounded-md p-4 {bg_class}",
            div { class: "flex",
                div { class: "flex-shrink-0",
                    // Icon based on type
                    match props.alert_type {
                        AlertType::Info => rsx! {
                            super::icons::InformationIcon { class: icon_class.to_string() }
                        },
                        AlertType::Success => rsx! {
                            super::icons::CheckIcon { class: icon_class.to_string() }
                        },
                        AlertType::Warning | AlertType::Error => rsx! {
                            super::icons::ExclamationIcon { class: icon_class.to_string() }
                        },
                    }
                }
                div { class: "ml-3 flex-1",
                    if !props.title.is_empty() {
                        h3 { class: "text-sm font-medium {text_class}",
                            "{props.title}"
                        }
                    }
                    p { class: "text-sm {text_class}",
                        "{props.message}"
                    }
                }
                if props.dismissible {
                    div { class: "ml-auto pl-3",
                        IconButton {
                            label: "Dismiss notification",
                            class: "-mx-1.5 -my-1.5 rounded-md p-1.5 inline-flex {text_class} hover:bg-surface-2 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-green-50 focus:ring-green-600",
                            onclick: move |_| props.ondismiss.call(()),
                            XMarkIcon {}
                        }
                    }
                }
            }
        }
    }
}

/// Toast notification position
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ToastPosition {
    #[default]
    BottomRight,
}

impl ToastPosition {
    fn class(&self) -> &'static str {
        match self {
            ToastPosition::BottomRight => "bottom-4 right-4",
        }
    }
}

/// Toast notification data
#[derive(Clone, PartialEq)]
pub struct Toast {
    pub id: String,
    pub toast_type: AlertType,
    pub message: String,
    pub title: Option<String>,
    /// MAPPS-312: auto-dismiss timeout in milliseconds. `None` keeps
    /// the toast sticky until the user clicks the dismiss button -
    /// the right behaviour for Warning / Error toasts where the user
    /// must see the failure even if they tabbed away. Success / Info
    /// toasts default to ~5s so the stack does not pile up across a
    /// normal session.
    pub auto_dismiss_ms: Option<u32>,
}

/// Toast container props
#[derive(Props, Clone, PartialEq)]
pub struct ToastContainerProps {
    toasts: Vec<Toast>,
    #[props(default)]
    position: ToastPosition,
    ondismiss: EventHandler<String>,
}

/// Toast container component
#[component]
pub fn ToastContainer(props: ToastContainerProps) -> Element {
    let position_class = props.position.class();

    rsx! {
        div { class: "fixed z-50 {position_class} space-y-2 w-80",
            for toast in props.toasts.iter() {
                ToastRow {
                    key: "{toast.id}",
                    toast: toast.clone(),
                    ondismiss: props.ondismiss,
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ToastRowProps {
    toast: Toast,
    ondismiss: EventHandler<String>,
}

/// MAPPS-312: one toast row, owning a per-toast auto-dismiss timer.
/// Splitting the row into its own component is what lets `use_effect`
/// scope to a single id - a `use_effect` in `ToastContainer` would
/// fire once per toast list change and either spawn duplicates or
/// schedule against the wrong id.
#[component]
fn ToastRow(props: ToastRowProps) -> Element {
    let id = props.toast.id.clone();
    let dismiss = props.ondismiss;
    let auto_dismiss_ms = props.toast.auto_dismiss_ms;
    // Per-id timer effect. Runs on first mount with this key (the
    // parent `for` iterates with `key: "{toast.id}"` so a fresh push
    // mounts a fresh component) and fires the dismiss after the
    // configured ms. `None` skips scheduling entirely - the toast
    // stays sticky until the user clicks X.
    use_effect(move || {
        let id = id.clone();
        if let Some(ms) = auto_dismiss_ms {
            #[cfg(feature = "app")]
            {
                dioxus::prelude::spawn(async move {
                    crate::platform::timer::sleep_ms(ms).await;
                    dismiss.call(id);
                });
            }
            #[cfg(not(feature = "app"))]
            let _ = (ms, id, dismiss);
        }
    });
    let dismiss_id = props.toast.id.clone();
    rsx! {
        div {
            class: "transform transition-all duration-300 ease-in-out",
            Alert {
                alert_type: props.toast.toast_type,
                title: props.toast.title.clone().unwrap_or_default(),
                message: props.toast.message.clone(),
                dismissible: true,
                ondismiss: move |_| dismiss.call(dismiss_id.clone()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::confirm_phrase_satisfied;
    use super::*;

    /// PMS-763: a modal whose chrome includes a tab bar. Rendered through the
    /// REAL [`ModalChrome`], so the test moves when the component does.
    #[component]
    fn WithSubheader() -> Element {
        rsx! {
            ModalChrome {
                title: "Title".to_string(),
                size: ModalSize::Large,
                onclose: move |_| {},
                subheader: rsx! { nav { "SUBHEADER-MARKER" } },
                "BODY-MARKER"
            }
        }
    }

    #[component]
    fn WithoutSubheader() -> Element {
        rsx! {
            ModalChrome {
                title: "Title".to_string(),
                size: ModalSize::Large,
                onclose: move |_| {},
                "BODY-MARKER"
            }
        }
    }

    /// PMS-763 regression: the request-form builder's tab bar used to be a
    /// `sticky` element at the top of the scrolling body, and a sticky box is
    /// constrained by the scrollport inset by the scroll container's padding.
    /// It therefore parked 16px below the top of the visible area and the field
    /// list scrolled through the strip above it. A subheader must render
    /// BEFORE the scrolling region, not inside it, so no such strip exists.
    #[test]
    fn a_subheader_is_pinned_outside_the_scrolling_body() {
        let mut dom = VirtualDom::new(WithSubheader);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);

        let subheader = html
            .find("SUBHEADER-MARKER")
            .expect("the subheader renders");
        // The BODY's scroller specifically. The outermost wrapper is also
        // `overflow-y-auto` (it scrolls the whole overlay on a short viewport),
        // and it opens the document, so a bare search for that class would
        // compare against the wrong box and pass no matter where the subheader
        // sat.
        let scroller = html
            .find("flex-1 min-h-0 overflow-y-auto")
            .expect("the body is still the scrolling region");
        let body = html.find("BODY-MARKER").expect("the body renders");

        assert!(
            subheader < scroller,
            "the subheader must sit above the scroll container, not inside it; got: {html}"
        );
        assert!(scroller < body, "the body content is inside the scroller");
        assert!(
            html[..subheader].contains("flex-shrink-0"),
            "the subheader is pinned like the header and footer; got: {html}"
        );
    }

    /// The slot is optional, and a modal without one must render exactly what
    /// it did before: no empty band, no second rule under the title.
    #[test]
    fn no_subheader_means_no_extra_chrome() {
        let mut dom = VirtualDom::new(WithoutSubheader);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);

        assert!(!html.contains("SUBHEADER-MARKER"));
        assert_eq!(
            html.matches("border-b border-line").count(),
            1,
            "only the header's own rule, no empty band under the title; got: {html}"
        );
    }

    // PMS-369: type-to-confirm gate. The destructive button is enabled only
    // when `confirm_phrase_satisfied` returns true.
    #[test]
    fn ungated_when_required_blank() {
        assert!(confirm_phrase_satisfied("", ""));
        assert!(confirm_phrase_satisfied("anything", "   "));
    }

    /// MAPPS-574: a dialog whose confirmed action was refused. Rendered
    /// through the REAL `ConfirmDialog`, so the test moves when it does.
    #[component]
    fn RefusedDialog() -> Element {
        rsx! {
            ConfirmDialog {
                open: true,
                title: "Delete company".to_string(),
                message: "BODY-MARKER".to_string(),
                destructive: true,
                confirm_phrase: "Acme Corp".to_string(),
                error: "Cannot delete company with existing tickets".to_string(),
                onconfirm: move |_| {},
                oncancel: move |_| {},
            }
        }
    }

    #[component]
    fn AcceptedDialog() -> Element {
        rsx! {
            ConfirmDialog {
                open: true,
                title: "Delete company".to_string(),
                message: "BODY-MARKER".to_string(),
                destructive: true,
                confirm_phrase: "Acme Corp".to_string(),
                onconfirm: move |_| {},
                oncancel: move |_| {},
            }
        }
    }

    #[component]
    fn ClosedDialog() -> Element {
        rsx! {
            ConfirmDialog {
                open: false,
                title: "Delete company".to_string(),
                message: "BODY-MARKER".to_string(),
                confirm_phrase: "Acme Corp".to_string(),
                error: "Cannot delete company with existing tickets".to_string(),
                onconfirm: move |_| {},
                oncancel: move |_| {},
            }
        }
    }

    fn render(app: fn() -> Element) -> String {
        let mut dom = VirtualDom::new(app);
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// MAPPS-574: the server's reason for refusing reaches the user. Before
    /// this, the company delete ran `.is_ok()` on the result and a refused
    /// delete closed the dialog saying nothing at all, so the reason was
    /// visible only in devtools.
    #[test]
    fn a_refusal_is_rendered_inside_the_dialog() {
        let html = render(RefusedDialog);
        assert!(
            html.contains("Cannot delete company with existing tickets"),
            "the server's own message must be shown verbatim; got: {html}"
        );
        assert!(
            html.contains(r#"role="alert""#),
            "a refusal arriving into an already-open dialog must be announced,              not merely drawn; got: {html}"
        );
    }

    /// The reason sits above the phrase input, so it is read before the control
    /// the user is about to reuse rather than after it.
    #[test]
    fn the_refusal_precedes_the_phrase_input() {
        let html = render(RefusedDialog);
        let banner = html
            .find("Cannot delete company with existing tickets")
            .expect("the refusal renders");
        let input = html.find("<input").expect("the phrase input renders");
        assert!(
            banner < input,
            "the reason must come before the input it explains; got: {html}"
        );
    }

    /// The slot is optional, and a dialog with nothing to report must render
    /// exactly what it did before: no empty banner above the phrase box.
    #[test]
    fn no_refusal_means_no_banner() {
        let html = render(AcceptedDialog);
        assert!(html.contains("BODY-MARKER"), "the dialog still renders");
        assert!(
            !html.contains(r#"role="alert""#),
            "an empty error must not render a banner; got: {html}"
        );
    }

    /// MAPPS-574: closing renders nothing, which is what makes the phrase state
    /// unable to survive into the next open. Pinned as behaviour so the
    /// early-return cannot be "simplified" back into passing `open` down.
    #[test]
    fn a_closed_dialog_renders_nothing() {
        let html = render(ClosedDialog);
        assert!(
            html.is_empty(),
            "a closed ConfirmDialog must render nothing at all; got: {html}"
        );
    }

    /// The re-arm itself: `typed` must be owned by the component that mounts
    /// with the dialog, never by `ConfirmDialog`, which the caller keeps
    /// mounted across closes. Held there, the phrase survived a cancel and the
    /// destructive button came back already enabled, so every attempt after the
    /// first was a one-click delete. That is a lifecycle property, and the DOM
    /// transition it depends on is not reachable from the host test harness
    /// (no wasm/browser runner; see docs/destructive-actions.md), so the
    /// ownership is asserted where it is decided.
    #[test]
    fn the_phrase_state_is_owned_by_the_open_dialog_only() {
        const SRC: &str = include_str!("modal.rs");
        let start = SRC
            .find("pub fn ConfirmDialog(")
            .expect("ConfirmDialog is defined here");
        let end = SRC
            .find("fn OpenConfirmDialog(")
            .expect("the open dialog is a separate component");
        assert!(start < end, "OpenConfirmDialog follows ConfirmDialog");
        let wrapper = &SRC[start..end];
        assert!(
            !wrapper.contains("use_signal"),
            "ConfirmDialog does not unmount when the dialog closes, so any state              it holds outlives the attempt it belongs to. Put it in              OpenConfirmDialog, which mounts with the dialog. Found in: {wrapper}"
        );
    }

    /// MAPPS-577: a refused action offers no gate and no confirm button.
    ///
    /// Rendered through the REAL `ConfirmDialog`. Asking somebody to type a
    /// company name to enable a button that will be refused is exactly the
    /// wasted effort the issue reported, and a DISABLED button is not enough:
    /// it invites the user to work out what would enable it.
    #[component]
    fn BlockedDialog() -> Element {
        rsx! {
            ConfirmDialog {
                open: true,
                title: "Delete company".to_string(),
                message: "BODY-MARKER".to_string(),
                confirm_text: "Delete".to_string(),
                destructive: true,
                blocked: true,
                confirm_phrase: "Acme Corp".to_string(),
                body: rsx! { p { "2 invoices" } },
                alternative: rsx! { button { "Archive instead" } },
                onconfirm: move |_| {},
                oncancel: move |_| {},
            }
        }
    }

    #[test]
    fn a_blocked_dialog_offers_no_gate_and_no_confirm() {
        let html = render(BlockedDialog);
        assert!(
            !html.contains("<input"),
            "no phrase gate: there is nothing for it to unlock; got: {html}"
        );
        assert!(
            !html.contains("to confirm"),
            "and no instruction to type one; got: {html}"
        );
        assert!(
            !html.contains(">Delete<"),
            "the confirm button is withheld, not disabled; got: {html}"
        );
        assert!(html.contains("Archive instead"), "{html}");
        assert!(html.contains("2 invoices"), "the body renders: {html}");
    }

    /// And the ordinary path is untouched: a deletable company still gets its
    /// gate and its Delete button. This must not become a two-step flow for the
    /// common case.
    #[component]
    fn AllowedWithBody() -> Element {
        rsx! {
            ConfirmDialog {
                open: true,
                title: "Delete company".to_string(),
                message: "BODY-MARKER".to_string(),
                confirm_text: "Delete".to_string(),
                destructive: true,
                confirm_phrase: "Acme Corp".to_string(),
                body: rsx! { p { "3 sites" } },
                onconfirm: move |_| {},
                oncancel: move |_| {},
            }
        }
    }

    #[test]
    fn an_allowed_dialog_keeps_its_gate_and_gains_the_body() {
        let html = render(AllowedWithBody);
        assert!(html.contains("<input"), "the gate is intact: {html}");
        assert!(html.contains(">Delete<"), "{html}");
        assert!(
            html.contains("3 sites"),
            "and it can still say what the delete would do: {html}"
        );
    }

    #[test]
    fn gated_needs_exact_trimmed_caseinsensitive_match() {
        // Wrong / partial input keeps the gate closed.
        assert!(!confirm_phrase_satisfied("", "Acme Corp"));
        assert!(!confirm_phrase_satisfied("Acme", "Acme Corp"));
        assert!(!confirm_phrase_satisfied("Acme Corpp", "Acme Corp"));
        // Exact match (with surrounding whitespace / different case) passes.
        assert!(confirm_phrase_satisfied("Acme Corp", "Acme Corp"));
        assert!(confirm_phrase_satisfied("  acme corp  ", "Acme Corp"));
    }
}

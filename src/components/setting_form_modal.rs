//! Shared create/edit modal chrome for the settings-style editors (lookup
//! editors, rate cards, ...). Owns the footer (Delete / Cancel / Save) and
//! the error banner; the per-form fields are passed as `children`.

use dioxus::prelude::*;

use super::{Button, ButtonVariant};

#[derive(Props, Clone, PartialEq)]
pub struct SettingFormModalProps {
    pub title: String,
    pub is_edit: bool,
    pub saving: bool,
    pub deleting: bool,
    pub error: String,
    pub create_label: String,
    /// Whether the Delete button renders on an edit. Defaults to true;
    /// pass false for rows the server refuses to delete (e.g. system
    /// `is_system` lookups).
    #[props(default = true)]
    pub deletable: bool,
    /// Title shown on the destructive-confirmation dialog (MAPPS-189).
    #[props(default = "Confirm deletion".to_string())]
    pub delete_title: String,
    /// Message shown on the destructive-confirmation dialog (MAPPS-189).
    /// Override per editor for a more specific prompt.
    #[props(default = "Are you sure you want to delete this item? This action cannot be undone.".to_string())]
    pub delete_message: String,
    pub onclose: EventHandler<()>,
    pub onsave: EventHandler<MouseEvent>,
    /// Fired once the user confirms the delete in the styled dialog.
    pub ondelete: EventHandler<()>,
    pub children: Element,
}

#[component]
pub fn SettingFormModal(props: SettingFormModalProps) -> Element {
    let onclose = props.onclose;
    let onsave = props.onsave;
    let ondelete = props.ondelete;
    let is_edit = props.is_edit;
    let deletable = props.deletable;
    let deleting = props.deleting;

    // MAPPS-189: destructive deletes confirm through the styled, themed,
    // focus-trapped ConfirmDialog instead of the native window.confirm().
    // The footer Delete button only opens the dialog; the caller's
    // `ondelete` fires from the dialog's confirm action below.
    let mut confirming = use_signal(|| false);

    let footer = rsx! {
        if is_edit && deletable {
            Button {
                variant: ButtonVariant::Danger,
                loading: props.deleting,
                onclick: move |_| confirming.set(true),
                "Delete"
            }
        }
        div { class: "flex-1" }
        Button {
            variant: ButtonVariant::Secondary,
            onclick: move |_| onclose.call(()),
            "Cancel"
        }
        Button {
            variant: ButtonVariant::Primary,
            loading: props.saving,
            onclick: move |e| onsave.call(e),
            if is_edit { "Save Changes" } else { "{props.create_label}" }
        }
    };

    rsx! {
        crate::components::Modal {
            open: true,
            title: props.title.clone(),
            size: crate::components::ModalSize::Medium,
            onclose: move |_| onclose.call(()),
            footer,
            div { class: "space-y-4",
                if !props.error.is_empty() {
                    div {
                        class: "text-sm text-red-700 dark:text-red-300 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-900 rounded-md px-3 py-2",
                        "{props.error}"
                    }
                }
                {props.children}
            }
        }
        crate::components::ConfirmDialog {
            open: confirming(),
            title: props.delete_title.clone(),
            message: props.delete_message.clone(),
            confirm_text: "Delete".to_string(),
            cancel_text: "Cancel".to_string(),
            destructive: true,
            loading: deleting,
            onconfirm: move |_| {
                confirming.set(false);
                ondelete.call(());
            },
            oncancel: move |_| {
                if !deleting {
                    confirming.set(false);
                }
            },
        }
    }
}

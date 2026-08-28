//! MAPPS-619/620 (mokosh-branding prompts 003/004): reusable branding
//! form. Rendered as a `<Card>` body; two consumers:
//!
//! - staff-side `CompanyBrandingCard` on the Company detail page,
//!   which loads / saves via `PUT /contacts/companies/{id}`;
//! - contact-plane `ContactPortalBrandingPage`, which loads / saves
//!   via `PATCH /contact/companies/self/branding`.
//!
//! Both consumers pass in the CURRENT overrides + the MSP DEFAULTS
//! (raw tenant branding) so the "Inherits from MSP default: X" hint
//! on every field points at the correct fallback. The `on_save`
//! callback receives the new override block; the caller owns the
//! actual network call so this component stays consumer-agnostic.
//!
//! Fields shipped in phase-A (JSON only, no uploads): display name,
//! primary + secondary + background colors, support email / phone /
//! contact name. Logo + favicon + background image uploads land in
//! a follow-up commit (MAPPS-618 asset-store extension) once the
//! server has the multipart endpoints; the widget already reserves
//! space for the upload rows via commented-out placeholders so the
//! layout does not shift when they land.

use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Card};
use crate::hooks::branding::{CompanyBranding, TenantBranding};

#[derive(Props, Clone, PartialEq)]
pub struct BrandingEditorProps {
    /// The Company override values currently in force (may be all
    /// `None` if the Company has never customized).
    pub current: CompanyBranding,
    /// The tenant defaults this Company inherits from. Rendered
    /// under each field as "Inherits from MSP default: X" so the
    /// editor knows what a `Reset` produces.
    pub tenant_defaults: TenantBranding,
    /// Whether the editor is disabled (in-flight save, no permission,
    /// etc.). The parent owns the display gate; passing `true` here
    /// only greys the fields, does not hide them.
    #[props(default = false)]
    pub disabled: bool,
    /// Called with the FULL updated override block on Save. The
    /// caller is responsible for serializing + PATCHing.
    pub on_save: EventHandler<CompanyBranding>,
    /// Called when the user clicks "Reset every field". The caller
    /// PATCHes an empty object which the server merges as a no-op if
    /// there is nothing to clear; consumers that want a real "clear
    /// all" should PATCH `{everything: null}` themselves.
    #[props(default)]
    pub on_reset_all: Option<EventHandler<()>>,
}

/// Small helper: render a "Inherits from MSP default: <value>" hint
/// when the tenant side has a value AND the Company override for the
/// same field is empty.
fn hint(default: Option<&str>) -> String {
    match default {
        Some(v) if !v.is_empty() => format!("Inherits from MSP default: {v}"),
        _ => "No MSP default; portal falls back to the coded default.".to_string(),
    }
}

#[component]
pub fn BrandingEditor(props: BrandingEditorProps) -> Element {
    // Local editable state initialised from the incoming override
    // block. On Save we hand the full state back through the
    // `on_save` callback; on Reset we clear a single field.
    let mut display_name = use_signal(|| props.current.display_name.clone().unwrap_or_default());
    let mut primary_color =
        use_signal(|| props.current.primary_color.clone().unwrap_or_default());
    let mut secondary_color =
        use_signal(|| props.current.secondary_color.clone().unwrap_or_default());
    let mut background_color =
        use_signal(|| props.current.background_color.clone().unwrap_or_default());
    let mut support_email =
        use_signal(|| props.current.support_email.clone().unwrap_or_default());
    let mut support_phone =
        use_signal(|| props.current.support_phone.clone().unwrap_or_default());
    let mut support_contact_name =
        use_signal(|| props.current.support_contact_name.clone().unwrap_or_default());

    let defaults = props.tenant_defaults.clone();
    let on_save = props.on_save;
    let disabled = props.disabled;

    let submit = move |_| {
        let block = CompanyBranding {
            display_name: Some(display_name.read().clone()).filter(|s| !s.is_empty()),
            primary_color: Some(primary_color.read().clone()).filter(|s| !s.is_empty()),
            secondary_color: Some(secondary_color.read().clone()).filter(|s| !s.is_empty()),
            background_color: Some(background_color.read().clone()).filter(|s| !s.is_empty()),
            support_email: Some(support_email.read().clone()).filter(|s| !s.is_empty()),
            support_phone: Some(support_phone.read().clone()).filter(|s| !s.is_empty()),
            support_contact_name: Some(support_contact_name.read().clone())
                .filter(|s| !s.is_empty()),
            // The following are logo / favicon / background image
            // fields that a later commit wires to real upload widgets;
            // preserve whatever the caller passed in so a save from
            // this JSON-only editor does not blow away an existing
            // uploaded asset.
            logo_url: props.current.logo_url.clone(),
            logo_mime: props.current.logo_mime.clone(),
            favicon_url: props.current.favicon_url.clone(),
            favicon_mime: props.current.favicon_mime.clone(),
            background_url: props.current.background_url.clone(),
            background_mime: props.current.background_mime.clone(),
            company_name: props.current.company_name.clone(),
            portal_domain: props.current.portal_domain.clone(),
        };
        on_save.call(block);
    };

    rsx! {
        Card { title: "Portal branding",
            div { class: "space-y-6",
                p { class: "text-sm text-muted",
                    "Customize how this Company's portal looks to its contacts. Empty fields inherit from the MSP-level defaults; the merged result is what the portal actually paints."
                }
                // Display name
                div { class: "space-y-1",
                    label {
                        r#for: "brand_display_name",
                        class: "block text-sm font-medium text-content",
                        "Display name"
                    }
                    input {
                        id: "brand_display_name",
                        r#type: "text",
                        class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                        placeholder: "Client Portal",
                        value: "{display_name}",
                        disabled,
                        oninput: move |e: FormEvent| display_name.set(e.value()),
                    }
                    p { class: "text-xs text-muted", "{hint(defaults.display_name.as_deref())}" }
                }
                // Colors
                div { class: "grid grid-cols-1 sm:grid-cols-3 gap-4",
                    div { class: "space-y-1",
                        label {
                            r#for: "brand_primary",
                            class: "block text-sm font-medium text-content",
                            "Primary color"
                        }
                        input {
                            id: "brand_primary",
                            r#type: "color",
                            class: "block h-10 w-full rounded-md border-line",
                            value: "{primary_color}",
                            disabled,
                            oninput: move |e: FormEvent| primary_color.set(e.value()),
                        }
                        p { class: "text-xs text-muted", "{hint(defaults.primary_color.as_deref())}" }
                    }
                    div { class: "space-y-1",
                        label {
                            r#for: "brand_secondary",
                            class: "block text-sm font-medium text-content",
                            "Secondary color"
                        }
                        input {
                            id: "brand_secondary",
                            r#type: "color",
                            class: "block h-10 w-full rounded-md border-line",
                            value: "{secondary_color}",
                            disabled,
                            oninput: move |e: FormEvent| secondary_color.set(e.value()),
                        }
                        p { class: "text-xs text-muted", "{hint(defaults.secondary_color.as_deref())}" }
                    }
                    div { class: "space-y-1",
                        label {
                            r#for: "brand_background",
                            class: "block text-sm font-medium text-content",
                            "Background color"
                        }
                        input {
                            id: "brand_background",
                            r#type: "color",
                            class: "block h-10 w-full rounded-md border-line",
                            value: "{background_color}",
                            disabled,
                            oninput: move |e: FormEvent| background_color.set(e.value()),
                        }
                        p { class: "text-xs text-muted", "{hint(defaults.background_color.as_deref())}" }
                    }
                }
                // Support contact block
                div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                    div { class: "space-y-1",
                        label {
                            r#for: "brand_support_email",
                            class: "block text-sm font-medium text-content",
                            "Support email"
                        }
                        input {
                            id: "brand_support_email",
                            r#type: "email",
                            class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                            placeholder: "support@example.com",
                            value: "{support_email}",
                            disabled,
                            oninput: move |e: FormEvent| support_email.set(e.value()),
                        }
                        p { class: "text-xs text-muted", "{hint(defaults.support_email.as_deref())}" }
                    }
                    div { class: "space-y-1",
                        label {
                            r#for: "brand_support_phone",
                            class: "block text-sm font-medium text-content",
                            "Support phone"
                        }
                        input {
                            id: "brand_support_phone",
                            r#type: "tel",
                            class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                            placeholder: "+1 555 555 5555",
                            value: "{support_phone}",
                            disabled,
                            oninput: move |e: FormEvent| support_phone.set(e.value()),
                        }
                        p { class: "text-xs text-muted", "{hint(defaults.support_phone.as_deref())}" }
                    }
                }
                div { class: "space-y-1",
                    label {
                        r#for: "brand_support_contact_name",
                        class: "block text-sm font-medium text-content",
                        "Support contact name"
                    }
                    input {
                        id: "brand_support_contact_name",
                        r#type: "text",
                        class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                        placeholder: "Alex from Ops",
                        value: "{support_contact_name}",
                        disabled,
                        oninput: move |e: FormEvent| support_contact_name.set(e.value()),
                    }
                    p { class: "text-xs text-muted", "{hint(defaults.support_contact_name.as_deref())}" }
                }
                // Save row
                div { class: "flex items-center justify-end gap-3 pt-4 border-t border-line",
                    Button {
                        variant: ButtonVariant::Primary,
                        disabled,
                        onclick: submit,
                        "Save branding"
                    }
                }
                // Placeholder note for the asset-upload rows that
                // land with the next server slice (MAPPS-618 uploads).
                p { class: "text-xs text-muted italic",
                    "Logo, favicon, and background image uploads are coming next; the underlying storage endpoints are queued behind this JSON editor."
                }
            }
        }
    }
}

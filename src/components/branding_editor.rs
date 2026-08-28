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
//! MAPPS-618 phase B added the three asset upload rows (logo,
//! favicon, background). Uploads go directly to the server via a
//! multipart PUT; the widget flips a `on_asset_saved` callback so
//! the parent can refetch the Company / branding block and repaint.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::{Button, ButtonVariant, Card};
use crate::hooks::branding::{CompanyBranding, TenantBranding};

/// Which plane the editor writes on. Drives the URL + the fetch
/// helper (staff bearer vs contact bearer) for asset uploads.
#[derive(Clone, PartialEq)]
pub enum BrandingPlane {
    /// MSP admin editing a specific Company under their tenant.
    /// `company_id` is the Company's UUID as a string (matches the
    /// URL segment `/api/v1/companies/{company_id}/{asset}`).
    Staff { company_id: String },
    /// Contact editing their own Company (server derives target
    /// Company from the session, so no id in the URL).
    ContactSelf,
}

impl BrandingPlane {
    fn asset_url(&self, asset: &str) -> String {
        match self {
            BrandingPlane::Staff { company_id } => {
                format!("/companies/{company_id}/{asset}")
            }
            BrandingPlane::ContactSelf => format!("/contact/companies/self/{asset}"),
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct BrandingEditorProps {
    /// The Company override values currently in force (may be all
    /// `None` if the Company has never customized).
    pub current: CompanyBranding,
    /// The tenant defaults this Company inherits from. Rendered
    /// under each field as "Inherits from MSP default: X" so the
    /// editor knows what a `Reset` produces.
    pub tenant_defaults: TenantBranding,
    /// Which plane the editor writes on (Staff vs ContactSelf).
    /// Drives the asset-upload URLs + the fetch helper choice.
    pub plane: BrandingPlane,
    /// Whether the editor is disabled (in-flight save, no permission,
    /// etc.). The parent owns the display gate; passing `true` here
    /// only greys the fields, does not hide them.
    #[props(default = false)]
    pub disabled: bool,
    /// Called with the FULL updated override block on Save. The
    /// caller is responsible for serializing + PATCHing.
    pub on_save: EventHandler<CompanyBranding>,
    /// Called on a successful asset upload or delete. The parent
    /// typically refetches the branding block so previews update to
    /// the fresh URL.
    #[props(default)]
    pub on_asset_saved: Option<EventHandler<()>>,
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

/// Kick off a multipart PUT of the picked file to the given URL,
/// using the appropriate bearer for the plane. Returns Ok on 2xx.
async fn upload_asset(
    plane: BrandingPlane,
    asset: &str,
    file: web_sys::File,
) -> Result<(), String> {
    let form = web_sys::FormData::new().map_err(|e| format!("{e:?}"))?;
    form.append_with_blob_and_filename("file", file.as_ref(), &file.name())
        .map_err(|e| format!("{e:?}"))?;
    let url = plane.asset_url(asset);
    match &plane {
        BrandingPlane::Staff { .. } => {
            crate::hooks::fetch::api::put_authed_multipart::<serde_json::Value>(&url, &form)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        BrandingPlane::ContactSelf => {
            crate::hooks::fetch::api::put_contact_authed_multipart::<serde_json::Value>(
                &url, &form,
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
        }
    }
}

async fn delete_asset(plane: BrandingPlane, asset: &str) -> Result<(), String> {
    let url = plane.asset_url(asset);
    match &plane {
        BrandingPlane::Staff { .. } => crate::hooks::fetch::api::delete_authed_typed(&url)
            .await
            .map_err(|e| e.to_string()),
        BrandingPlane::ContactSelf => {
            crate::hooks::fetch::api::delete_contact_authed_no_content(&url)
                .await
                .map_err(|e| e.to_string())
        }
    }
}

/// One row for a single asset (logo / favicon / background). Reads
/// current URL for the preview, offers a file picker + Remove
/// button. Fires `on_saved` on any successful mutation so the parent
/// can refetch + repaint.
#[component]
fn AssetUploadRow(
    label: String,
    asset: String,
    current_url: Option<String>,
    plane: BrandingPlane,
    disabled: bool,
    on_saved: EventHandler<()>,
) -> Element {
    let mut saving = use_signal(|| false);
    let mut error: Signal<String> = use_signal(String::new);
    let asset_for_upload = asset.clone();
    let plane_for_upload = plane.clone();
    let onchange = move |_evt: FormEvent| {
        // Grab the picked file straight off the DOM. dioxus 0.7's
        // `FormEvent::files()` returns a `FileEngine` that hides the
        // raw `web_sys::File`; a multipart upload needs the native
        // `Blob`, so bypass the engine and query the input by id.
        let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(&format!("brand_upload_{}", asset_for_upload)))
        else {
            error.set("Could not read the picked file.".to_string());
            return;
        };
        let Ok(input) = el.dyn_into::<web_sys::HtmlInputElement>() else {
            error.set("Could not read the picked file.".to_string());
            return;
        };
        let Some(file_list) = input.files() else {
            return;
        };
        let Some(file) = file_list.item(0) else {
            return;
        };
        saving.set(true);
        error.set(String::new());
        let asset = asset_for_upload.clone();
        let plane = plane_for_upload.clone();
        spawn(async move {
            match upload_asset(plane, &asset, file).await {
                Ok(_) => on_saved.call(()),
                Err(e) => error.set(format!("Upload failed: {e}")),
            }
            saving.set(false);
        });
    };
    let asset_for_remove = asset.clone();
    let plane_for_remove = plane.clone();
    let on_remove = move |_| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        let asset = asset_for_remove.clone();
        let plane = plane_for_remove.clone();
        spawn(async move {
            match delete_asset(plane, &asset).await {
                Ok(_) => on_saved.call(()),
                Err(e) => error.set(format!("Remove failed: {e}")),
            }
            saving.set(false);
        });
    };
    rsx! {
        div { class: "space-y-2",
            label { class: "block text-sm font-medium text-content", "{label}" }
            div { class: "flex items-center gap-4",
                if let Some(url) = current_url.as_ref() {
                    img {
                        src: "{url}",
                        alt: "{label} preview",
                        class: "h-16 w-16 rounded border border-line object-contain bg-surface",
                    }
                } else {
                    div {
                        class: "h-16 w-16 rounded border border-dashed border-line grid place-items-center text-xs text-muted bg-surface",
                        "No file"
                    }
                }
                div { class: "flex-1 space-y-2",
                    input {
                        id: "brand_upload_{asset}",
                        r#type: "file",
                        accept: "image/png,image/jpeg,image/webp,image/gif",
                        class: "block w-full text-sm text-content file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-accent file:text-on-accent hover:file:opacity-90",
                        disabled: disabled || saving(),
                        onchange,
                    }
                    if current_url.is_some() {
                        button {
                            r#type: "button",
                            class: "text-xs text-red-600 hover:underline",
                            disabled: disabled || saving(),
                            onclick: on_remove,
                            "Remove"
                        }
                    }
                }
            }
            if !error().is_empty() {
                p { role: "alert", class: "text-xs text-red-600 dark:text-red-400", "{error}" }
            }
        }
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
        // Only the JSON-owned fields land in the save block. Asset
        // fields (logo/favicon/background url+mime) are written by
        // the multipart upload rows above and stay `None` here so
        // `skip_serializing_if = Option::is_none` on the wire type
        // omits them, letting the server's `||` JSONB merge leave
        // whatever the uploads set alone. Explicit-clear for these
        // fields flows through the row's Remove button.
        let block = CompanyBranding {
            display_name: Some(display_name.read().clone()).filter(|s| !s.is_empty()),
            primary_color: Some(primary_color.read().clone()).filter(|s| !s.is_empty()),
            secondary_color: Some(secondary_color.read().clone()).filter(|s| !s.is_empty()),
            background_color: Some(background_color.read().clone()).filter(|s| !s.is_empty()),
            support_email: Some(support_email.read().clone()).filter(|s| !s.is_empty()),
            support_phone: Some(support_phone.read().clone()).filter(|s| !s.is_empty()),
            support_contact_name: Some(support_contact_name.read().clone())
                .filter(|s| !s.is_empty()),
            ..CompanyBranding::default()
        };
        on_save.call(block);
    };

    rsx! {
        Card { title: "Portal branding",
            div { class: "space-y-6",
                p { class: "text-sm text-muted",
                    "Customize how this Company's portal looks to its contacts. Empty fields inherit from the MSP-level defaults; the merged result is what the portal actually paints."
                }
                // Asset uploads (MAPPS-618 phase B). Each row shows
                // the current image (or a "No file" placeholder) +
                // a file picker + a Remove button. Uploads fire
                // multipart PUTs directly against the branding
                // routes; on success the parent refetches via
                // `on_asset_saved`.
                div { class: "space-y-4 pb-4 border-b border-line",
                    AssetUploadRow {
                        label: "Logo".to_string(),
                        asset: "logo".to_string(),
                        current_url: props.current.logo_url.clone(),
                        plane: props.plane.clone(),
                        disabled,
                        on_saved: move |_| {
                            if let Some(cb) = props.on_asset_saved.as_ref() {
                                cb.call(());
                            }
                        },
                    }
                    AssetUploadRow {
                        label: "Favicon".to_string(),
                        asset: "favicon".to_string(),
                        current_url: props.current.favicon_url.clone(),
                        plane: props.plane.clone(),
                        disabled,
                        on_saved: move |_| {
                            if let Some(cb) = props.on_asset_saved.as_ref() {
                                cb.call(());
                            }
                        },
                    }
                    AssetUploadRow {
                        label: "Background image".to_string(),
                        asset: "background".to_string(),
                        current_url: props.current.background_url.clone(),
                        plane: props.plane.clone(),
                        disabled,
                        on_saved: move |_| {
                            if let Some(cb) = props.on_asset_saved.as_ref() {
                                cb.call(());
                            }
                        },
                    }
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
            }
        }
    }
}

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
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

use crate::components::{Button, ButtonVariant, Card};
use crate::hooks::branding::{CompanyBranding, TenantBranding};

/// Which plane the editor writes on. Drives the URL + the fetch
/// helper (staff bearer vs contact bearer) for asset uploads.
#[derive(Clone, PartialEq)]
pub enum BrandingPlane {
    /// MSP admin editing the tenant-wide defaults. All Companies
    /// inherit these unless they override at Company scope. Writes
    /// to `/api/v1/tenants/current/branding/{asset}`.
    StaffTenant,
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
            BrandingPlane::StaffTenant => format!("/tenants/current/branding/{asset}"),
            BrandingPlane::Staff { company_id } => {
                format!("/companies/{company_id}/{asset}")
            }
            BrandingPlane::ContactSelf => format!("/contact/companies/self/{asset}"),
        }
    }
}

/// MAPPS-635 D4: compute the WCAG contrast ratio (1.0..21.0) between
/// a hex color string like `"#ff8800"` and pure black + pure white,
/// return the higher of the two. Poor readability against BOTH
/// black and white text (below the AA threshold of 4.5) is a strong
/// signal the chosen brand color needs adjustment. Returns `None`
/// for a bogus hex string so the caller can silently skip the
/// warning rather than block save on a parse error.
fn best_contrast_against_bw(hex: &str) -> Option<f32> {
    fn parse(hex: &str) -> Option<(u8, u8, u8)> {
        let s = hex.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some((r, g, b))
    }
    fn linearize(c: u8) -> f32 {
        let c = c as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let (r, g, b) = parse(hex)?;
    let l = 0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b);
    let vs_black = (l + 0.05) / 0.05;
    let vs_white = (1.05) / (l + 0.05);
    Some(vs_black.max(vs_white))
}

/// MAPPS-635 D4: WCAG AA passing threshold for normal-size text.
const WCAG_AA_NORMAL: f32 = 4.5;

/// MAPPS-635 D6: PATCH `{field: null}` to the branding endpoint for
/// the current plane, so a Company override falls back to the tenant
/// default (or, on the tenant page, to the coded default). Called
/// from the "Reset" text button under each color picker; native
/// `<input type="color">` has no empty state, so an explicit reset
/// affordance is the only way to unwind an override without a full
/// re-save.
async fn reset_field(plane: BrandingPlane, field: &str) -> Result<(), String> {
    let patch = serde_json::json!({ field: serde_json::Value::Null });
    match plane {
        BrandingPlane::StaffTenant => {
            let body = serde_json::json!({ "branding": patch });
            crate::hooks::fetch::api::put_authed_typed::<serde_json::Value, _>(
                "/tenants/current",
                &body,
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
        }
        BrandingPlane::Staff { company_id } => {
            let body = serde_json::json!({ "branding": patch });
            crate::hooks::fetch::api::put_authed_typed::<serde_json::Value, _>(
                &format!("/contacts/companies/{company_id}"),
                &body,
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
        }
        BrandingPlane::ContactSelf => crate::hooks::fetch::api::patch_contact_authed_typed::<
            serde_json::Value,
            _,
        >("/contact/companies/self/branding", &patch)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string()),
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

/// PMS-1338: whether this plane edits the support-contact fields.
///
/// It does everywhere EXCEPT the tenant plane, and the reason is that on the
/// tenant plane those three keys are not branding's to own. `support_phone`,
/// `support_email` and `support_contact_name` in `tenants.branding` ARE the
/// organisation record: `PUT /api/v1/tenants/current/organization` writes the
/// Organization page's phone, email and contact name into exactly those keys
/// (PMS-896). So the editor was a second editor of one record, with different
/// labels, and the Organization page's whole-record write nulls an optional key
/// it omits, which meant saving it wiped a contact name entered here.
///
/// One editor each, then: the organisation owns the tenant-level values, and the
/// per-Company and contact planes keep their fields because those genuinely are
/// overrides, resolved over the tenant's values by `effective_branding`.
pub(crate) fn edits_support_contact(plane: &BrandingPlane) -> bool {
    !matches!(plane, BrandingPlane::StaffTenant)
}

/// PMS-1338: the organisation's contact details in one line, for the tenant
/// plane's read-only block. Empty fields are left out rather than rendered as
/// gaps, and a record with nothing in it says so, because "Support contact:" on
/// its own reads as a bug rather than as an empty record.
pub(crate) fn support_contact_summary(name: &str, email: &str, phone: &str) -> String {
    let parts: Vec<&str> = [name, email, phone]
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        "Not set yet.".to_string()
    } else {
        parts.join(" - ")
    }
}

/// Render the hint line under each field. Company/Contact scopes
/// point at the tenant default (or "no default set"); Tenant scope
/// only has the coded fallback, so the hint reads that instead.
fn hint(plane: &BrandingPlane, default: Option<&str>) -> String {
    match plane {
        BrandingPlane::StaffTenant => "Blank falls back to the platform default.".to_string(),
        _ => match default {
            Some(v) if !v.is_empty() => format!("Inherits from MSP default: {v}"),
            _ => "No MSP default; portal falls back to the coded default.".to_string(),
        },
    }
}

/// Kick off a multipart PUT of the picked file to the given URL,
/// using the appropriate bearer for the plane. Returns Ok on 2xx.
#[cfg(target_arch = "wasm32")]
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
        BrandingPlane::StaffTenant | BrandingPlane::Staff { .. } => {
            crate::hooks::fetch::api::put_authed_multipart::<serde_json::Value>(&url, &form)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        BrandingPlane::ContactSelf => {
            crate::hooks::fetch::api::put_contact_authed_multipart::<serde_json::Value>(&url, &form)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }
}

async fn delete_asset(plane: BrandingPlane, asset: &str) -> Result<(), String> {
    let url = plane.asset_url(asset);
    match &plane {
        BrandingPlane::StaffTenant | BrandingPlane::Staff { .. } => {
            crate::hooks::fetch::api::delete_authed_typed(&url)
                .await
                .map_err(|e| e.to_string())
        }
        BrandingPlane::ContactSelf => {
            crate::hooks::fetch::api::delete_contact_authed_no_content(&url)
                .await
                .map_err(|e| e.to_string())
        }
    }
}

/// MAPPS-635 D6: color picker row with a "Reset" link that PATCHes
/// `{field: null}` on the current plane so the value falls back to
/// the tenant default (or, on the tenant plane, the coded default).
/// Only renders the Reset link when the current props say an
/// override is present, so a Company that has no override in force
/// doesn't offer a no-op click.
#[component]
fn ColorField(
    id: String,
    label: String,
    field: String,
    value: Signal<String>,
    hint: String,
    plane: BrandingPlane,
    disabled: bool,
    override_present: bool,
    on_reset_saved: EventHandler<()>,
) -> Element {
    let mut saving = use_signal(|| false);
    let mut error: Signal<String> = use_signal(String::new);
    let field_for_reset = field.clone();
    let plane_for_reset = plane.clone();
    let on_reset = move |_| {
        if saving() {
            return;
        }
        saving.set(true);
        error.set(String::new());
        let field = field_for_reset.clone();
        let plane = plane_for_reset.clone();
        let mut value = value;
        spawn(async move {
            match reset_field(plane, &field).await {
                Ok(_) => {
                    value.set(String::new());
                    on_reset_saved.call(());
                }
                Err(_) => error.set("Reset failed. Try again in a moment.".to_string()),
            }
            saving.set(false);
        });
    };
    rsx! {
        div { class: "space-y-1",
            label {
                r#for: "{id}",
                class: "block text-sm font-medium text-content",
                "{label}"
            }
            input {
                id: "{id}",
                r#type: "color",
                class: "block h-10 w-full rounded-md border-line",
                value: "{value}",
                disabled: disabled || saving(),
                oninput: {
                    let mut value = value;
                    move |e: FormEvent| value.set(e.value())
                },
            }
            p { class: "text-xs text-muted", "{hint}" }
            if override_present {
                button {
                    r#type: "button",
                    class: "text-xs text-accent hover:underline disabled:opacity-50",
                    disabled: disabled || saving(),
                    onclick: on_reset,
                    "Reset to inherit"
                }
            }
            if !error().is_empty() {
                p { role: "alert", class: "text-xs text-red-600 dark:text-red-400", "{error}" }
            }
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
        #[cfg(target_arch = "wasm32")]
        {
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
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (
                &_evt,
                &asset_for_upload,
                &plane_for_upload,
                &mut saving,
                &mut error,
                &on_saved,
            );
        }
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
                    crate::components::FileField {
                        name: format!("brand_upload_{asset}"),
                        accept: "image/png,image/jpeg,image/webp,image/gif".to_string(),
                        disabled: disabled || saving(),
                        onchange,
                    }
                    if current_url.is_some() {
                        button {
                            r#type: "button",
                            class: "text-xs text-red-600 hover:underline dark:text-red-400",
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
    // MAPPS-1024: the invoice-identity block. `company_name`,
    // `legal_name`, `postal_address` and `tax_id` are shown on every
    // plane; `invoice_template` is StaffTenant-only (TENANT_ONLY_KEYS
    // on the server).
    let mut company_name = use_signal(|| props.current.company_name.clone().unwrap_or_default());
    let mut legal_name = use_signal(|| props.current.legal_name.clone().unwrap_or_default());
    let mut postal_address =
        use_signal(|| props.current.postal_address.clone().unwrap_or_default());
    let mut tax_id = use_signal(|| props.current.tax_id.clone().unwrap_or_default());
    let mut invoice_template =
        use_signal(|| props.current.invoice_template.clone().unwrap_or_default());
    let primary_color = use_signal(|| props.current.primary_color.clone().unwrap_or_default());
    let secondary_color = use_signal(|| props.current.secondary_color.clone().unwrap_or_default());
    let background_color =
        use_signal(|| props.current.background_color.clone().unwrap_or_default());
    let mut support_email = use_signal(|| props.current.support_email.clone().unwrap_or_default());
    let mut support_phone = use_signal(|| props.current.support_phone.clone().unwrap_or_default());
    let mut support_contact_name = use_signal(|| {
        props
            .current
            .support_contact_name
            .clone()
            .unwrap_or_default()
    });

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
            company_name: Some(company_name.read().clone()).filter(|s| !s.is_empty()),
            legal_name: Some(legal_name.read().clone()).filter(|s| !s.is_empty()),
            postal_address: Some(postal_address.read().clone()).filter(|s| !s.is_empty()),
            tax_id: Some(tax_id.read().clone()).filter(|s| !s.is_empty()),
            invoice_template: Some(invoice_template.read().clone()).filter(|s| !s.is_empty()),
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

    let intro_copy = match &props.plane {
        BrandingPlane::StaffTenant => {
            "MSP-wide defaults. Every Company under your tenant inherits these values unless it overrides them at Company scope. What contacts see on the login shell and inside the portal reads back through the tenant → Company merge."
        }
        BrandingPlane::Staff { .. } => {
            "Customize how this Company's portal looks to its contacts. Empty fields inherit from the MSP-level defaults; the merged result is what the portal actually paints."
        }
        BrandingPlane::ContactSelf => {
            "Customize how your Company's portal looks. Empty fields inherit from your MSP's defaults; the merged result is what your colleagues see."
        }
    };
    rsx! {
        Card { title: "Portal branding",
            div { class: "space-y-6",
                p { class: "text-sm text-muted", "{intro_copy}" }
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
                        // MAPPS-635 A: version the preview URL from
                        // the current branding block so a fresh
                        // upload evicts the cached bytes on the very
                        // next parent restart.
                        current_url: props.current.logo_url.clone().map(|u|
                            crate::hooks::branding::absolute_versioned_asset_url(&u, &props.current)
                        ),
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
                        current_url: props.current.favicon_url.clone().map(|u|
                            crate::hooks::branding::absolute_versioned_asset_url(&u, &props.current)
                        ),
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
                        current_url: props.current.background_url.clone().map(|u|
                            crate::hooks::branding::absolute_versioned_asset_url(&u, &props.current)
                        ),
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
                    p { class: "text-xs text-muted", "{hint(&props.plane, defaults.display_name.as_deref())}" }
                }
                // MAPPS-1024: business identity. The invoice "From"
                // block picks the first non-empty of `legal_name`
                // then `company_name` then the tenant display name
                // (PMS-911 issuer::resolve). `invoice_template`
                // (PMS-1006) is tenant-only on the server; the
                // override planes hide the select.
                div { class: "space-y-4 pt-4 border-t border-line",
                    p { class: "text-sm font-medium text-content", "Business identity" }
                    p { class: "text-xs text-muted",
                        "What your invoices read as under \"From\". Blank legal name falls through to trading name, then to the tenant's display name."
                    }
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                        div { class: "space-y-1",
                            label {
                                r#for: "brand_company_name",
                                class: "block text-sm font-medium text-content",
                                "Trading name"
                            }
                            input {
                                id: "brand_company_name",
                                r#type: "text",
                                class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                                placeholder: "Acme IT",
                                value: "{company_name}",
                                disabled,
                                oninput: move |e: FormEvent| company_name.set(e.value()),
                            }
                            p { class: "text-xs text-muted", "{hint(&props.plane, defaults.company_name.as_deref())}" }
                        }
                        div { class: "space-y-1",
                            label {
                                r#for: "brand_legal_name",
                                class: "block text-sm font-medium text-content",
                                "Legal name"
                            }
                            input {
                                id: "brand_legal_name",
                                r#type: "text",
                                class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                                placeholder: "Acme IT Services Pty Ltd",
                                value: "{legal_name}",
                                disabled,
                                oninput: move |e: FormEvent| legal_name.set(e.value()),
                            }
                            p { class: "text-xs text-muted", "{hint(&props.plane, defaults.legal_name.as_deref())}" }
                        }
                    }
                    div { class: "space-y-1",
                        label {
                            r#for: "brand_tax_id",
                            class: "block text-sm font-medium text-content",
                            "Tax ID"
                        }
                        input {
                            id: "brand_tax_id",
                            r#type: "text",
                            class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                            placeholder: "ABN 12 345 678 901",
                            value: "{tax_id}",
                            disabled,
                            oninput: move |e: FormEvent| tax_id.set(e.value()),
                        }
                        p { class: "text-xs text-muted", "{hint(&props.plane, defaults.tax_id.as_deref())}" }
                    }
                    div { class: "space-y-1",
                        label {
                            r#for: "brand_postal_address",
                            class: "block text-sm font-medium text-content",
                            "Postal address"
                        }
                        textarea {
                            id: "brand_postal_address",
                            class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                            rows: "4",
                            placeholder: "12 Example Street\nSuite 4\nSydney NSW 2000",
                            value: "{postal_address}",
                            disabled,
                            oninput: move |e: FormEvent| postal_address.set(e.value()),
                        }
                        p { class: "text-xs text-muted",
                            "One line per address line. Up to six lines. {hint(&props.plane, defaults.postal_address.as_deref())}"
                        }
                    }
                    if matches!(props.plane, BrandingPlane::StaffTenant) {
                        div { class: "space-y-1",
                            label {
                                r#for: "brand_invoice_template",
                                class: "block text-sm font-medium text-content",
                                "Invoice template"
                            }
                            select {
                                id: "brand_invoice_template",
                                class: "block w-full rounded-md border-line shadow-sm focus:border-accent focus:ring-accent bg-surface text-content sm:text-sm",
                                value: "{invoice_template}",
                                disabled,
                                onchange: move |e: FormEvent| invoice_template.set(e.value()),
                                option { value: "", "Default (classic)" }
                                option { value: "classic", "Classic" }
                                option { value: "modern", "Modern" }
                                option { value: "compact", "Compact" }
                            }
                            p { class: "text-xs text-muted",
                                "Picks the PDF layout for every invoice this tenant issues. Takes effect at next invoice send; already-sent invoices keep their frozen template."
                            }
                        }
                    }
                }
                // Colors + a per-field "Reset" affordance
                // (MAPPS-635 D6). Native `<input type="color">` has no
                // empty state so a Company override cannot be cleared
                // by editing the picker alone; each Reset button
                // fires a PATCH that nulls that specific color key,
                // and the parent's `on_asset_saved` refetches +
                // toasts.
                div { class: "grid grid-cols-1 sm:grid-cols-3 gap-4",
                    ColorField {
                        id: "brand_primary".to_string(),
                        label: "Primary color".to_string(),
                        field: "primary_color".to_string(),
                        value: primary_color,
                        hint: hint(&props.plane, defaults.primary_color.as_deref()),
                        plane: props.plane.clone(),
                        disabled,
                        override_present: props.current.primary_color.as_deref().map(|s| !s.is_empty()).unwrap_or(false),
                        on_reset_saved: move |_| {
                            if let Some(cb) = props.on_asset_saved.as_ref() {
                                cb.call(());
                            }
                        },
                    }
                    ColorField {
                        id: "brand_secondary".to_string(),
                        label: "Secondary color".to_string(),
                        field: "secondary_color".to_string(),
                        value: secondary_color,
                        hint: hint(&props.plane, defaults.secondary_color.as_deref()),
                        plane: props.plane.clone(),
                        disabled,
                        override_present: props.current.secondary_color.as_deref().map(|s| !s.is_empty()).unwrap_or(false),
                        on_reset_saved: move |_| {
                            if let Some(cb) = props.on_asset_saved.as_ref() {
                                cb.call(());
                            }
                        },
                    }
                    ColorField {
                        id: "brand_background".to_string(),
                        label: "Background color".to_string(),
                        field: "background_color".to_string(),
                        value: background_color,
                        hint: hint(&props.plane, defaults.background_color.as_deref()),
                        plane: props.plane.clone(),
                        disabled,
                        override_present: props.current.background_color.as_deref().map(|s| !s.is_empty()).unwrap_or(false),
                        on_reset_saved: move |_| {
                            if let Some(cb) = props.on_asset_saved.as_ref() {
                                cb.call(());
                            }
                        },
                    }
                }
                // MAPPS-635 D4: soft WCAG AA contrast warning. Every
                // chosen color that reads at less than 4.5:1 against
                // both black + white text (the two colors the SPA
                // draws over the brand primary / background) is
                // flagged. Advisory only - not a save blocker; the
                // MSP might have their reasons.
                {
                    let mut low = Vec::new();
                    for (label, sig) in [
                        ("Primary", primary_color.read().clone()),
                        ("Secondary", secondary_color.read().clone()),
                        ("Background", background_color.read().clone()),
                    ] {
                        if let Some(ratio) = best_contrast_against_bw(&sig) {
                            if ratio < WCAG_AA_NORMAL {
                                low.push(format!("{label} ({ratio:.1}:1)"));
                            }
                        }
                    }
                    if !low.is_empty() {
                        let joined = low.join(", ");
                        rsx! {
                            p {
                                role: "note",
                                class: "text-xs text-amber-600 dark:text-amber-400",
                                "Contrast note: {joined} may not be readable against dark/light text (WCAG AA needs 4.5:1). Consider a bolder color."
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
                // Support contact block. PMS-1338: on the tenant plane these
                // three keys are the organisation record, so the editor shows
                // them and sends the operator to the one place that writes them.
                if !edits_support_contact(&props.plane) {
                    div { class: "space-y-1 rounded-md border border-line bg-surface-2 p-4",
                        p { class: "text-sm font-medium text-content", "Support contact" }
                        p { class: "text-sm text-muted",
                            {support_contact_summary(
                                support_contact_name.read().as_str(),
                                support_email.read().as_str(),
                                support_phone.read().as_str(),
                            )}
                        }
                        p { class: "text-xs text-muted",
                            "These are your organisation's contact details, not a separate branding value: the portal, your invoices and your outbound mail all read the same record. "
                            Link {
                                to: crate::Route::SettingsOrganization {},
                                class: "text-accent hover:underline",
                                "Edit them under Organization"
                            }
                            "."
                        }
                        p { class: "text-xs text-muted",
                            "A single client portal can still say something different: set a support contact on that Company, which wins over this."
                        }
                    }
                }
                if edits_support_contact(&props.plane) {
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
                        p { class: "text-xs text-muted", "{hint(&props.plane, defaults.support_email.as_deref())}" }
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
                        p { class: "text-xs text-muted", "{hint(&props.plane, defaults.support_phone.as_deref())}" }
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
                    p { class: "text-xs text-muted", "{hint(&props.plane, defaults.support_contact_name.as_deref())}" }
                }
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

#[cfg(test)]
mod tests {
    use super::{edits_support_contact, support_contact_summary, BrandingPlane};

    /// PMS-1338: the tenant plane does not edit the support contact, and the two
    /// override planes do.
    ///
    /// The asymmetry is the whole decision, so it is asserted rather than left to
    /// a reader of the rsx: on the tenant plane those three keys ARE the
    /// organisation record that `PUT /tenants/current/organization` writes
    /// (PMS-896), and a Company or a contact editing them is a genuine override
    /// that `effective_branding` resolves over the tenant's values.
    #[test]
    fn only_the_override_planes_edit_the_support_contact() {
        assert!(!edits_support_contact(&BrandingPlane::StaffTenant));
        assert!(edits_support_contact(&BrandingPlane::Staff {
            company_id: "c0ffee".to_string()
        }));
        assert!(edits_support_contact(&BrandingPlane::ContactSelf));
    }

    /// The read-only line on the tenant plane. An empty record says so, because
    /// "Support contact:" followed by nothing reads as a bug rather than as a
    /// record nobody has filled in.
    #[test]
    fn the_summary_names_what_is_set_and_says_when_nothing_is() {
        assert_eq!(
            support_contact_summary("Alex from Ops", "support@acme.example", "+1 555 555 5555"),
            "Alex from Ops - support@acme.example - +1 555 555 5555"
        );
        assert_eq!(
            support_contact_summary("", "support@acme.example", ""),
            "support@acme.example"
        );
        assert_eq!(support_contact_summary("  ", "", "\t"), "Not set yet.");
        assert_eq!(support_contact_summary("", "", ""), "Not set yet.");
    }

    /// PMS-1338: hiding the inputs must not clear the keys. The save block is
    /// built from the same signals whether or not the fields render, so a tenant
    /// save carries the organisation's values through untouched; if it sent
    /// `None` instead, saving a logo would null the organisation's phone number,
    /// which is the clobber this change exists to close coming back the other
    /// way.
    #[test]
    fn the_tenant_save_still_carries_the_contact_keys() {
        let src = include_str!("branding_editor.rs");
        let submit = src
            .find("let submit = move |_| {")
            .expect("the save closure is in this file");
        let block_end = src[submit..]
            .find("on_save.call(block);")
            .expect("the closure calls on_save");
        let block = &src[submit..submit + block_end];
        for key in [
            "support_email:",
            "support_phone:",
            "support_contact_name:",
            // MAPPS-1024: invoice-identity keys drive the invoice
            // "From" line and the template choice. They sit on
            // every plane's save block so a plane-conditional UI
            // can never silently drop them on save.
            "company_name:",
            "legal_name:",
            "postal_address:",
            "tax_id:",
            "invoice_template:",
        ] {
            assert!(
                block.contains(key),
                "{key} left the save block, so a tenant save would clear it"
            );
        }
        assert!(
            !block.contains("edits_support_contact"),
            "the save block must not depend on whether the fields render"
        );
    }
}

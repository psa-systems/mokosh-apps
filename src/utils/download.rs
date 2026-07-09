//! Client-side "save bytes as a file" helper (MAPPS-364).
//!
//! The SPA holds the bearer token in WASM memory, so an attachment endpoint
//! cannot be reached with a plain `<a href>` navigation (it carries no
//! Authorization header). The page fetches the bytes with the bearer via
//! [`crate::hooks::fetch::api::get_authed_bytes`] and hands them here to trigger
//! a browser download.

/// Save `bytes` to the user's machine as `filename` by wrapping them in a Blob
/// and clicking a synthesized anchor over an object URL. Returns an error
/// string if any web-sys step fails.
#[cfg(feature = "web")]
pub fn save_bytes_as_file(bytes: &[u8], filename: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|_| "could not build the download blob".to_string())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| "could not create the download URL".to_string())?;

    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| "no document available for the download".to_string())?;
    let anchor = document
        .create_element("a")
        .map_err(|_| "could not create the download anchor".to_string())?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "download anchor cast failed".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    // Attach the anchor (hidden) before clicking: a detached-anchor click is
    // not honored in every browser. Remove it again afterwards.
    let _ = anchor.style().set_property("display", "none");
    let body = document
        .body()
        .ok_or_else(|| "no document body for the download".to_string())?;
    let _ = body.append_child(&anchor);
    anchor.click();
    let _ = body.remove_child(&anchor);

    // The browser has taken the blob into its download pipeline, so the object
    // URL can be released.
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

/// Non-web stub so call sites compile under `cargo check` without the `web`
/// feature.
#[cfg(not(feature = "web"))]
pub fn save_bytes_as_file(_bytes: &[u8], _filename: &str) -> Result<(), String> {
    Err("file download is only available in the browser".to_string())
}

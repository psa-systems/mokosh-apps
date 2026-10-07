//! Handing the user a file (MAPPS-504).
//!
//! The bytes are already in memory: the export endpoint needs the
//! bearer token, so the page fetches it itself rather than letting an
//! `<a href>` navigate (MAPPS-364).
//!
//! `Ok(None)` means the host took the file and will tell the user where
//! it went - that is the browser's download shelf. `Ok(Some(path))`
//! means this code chose the destination, so the caller has to show it;
//! a file that silently appears somewhere the user cannot find has not
//! been delivered.

/// Save `bytes` as `filename`.
#[cfg(target_arch = "wasm32")]
pub fn save_bytes_as_file(bytes: &[u8], filename: &str) -> Result<Option<String>, String> {
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
    Ok(None)
}

/// Write to the per-user downloads directory, falling back to the
/// documents directory and then the home directory. An existing file of
/// the same name is not overwritten: a numeric suffix is added, the way
/// a browser does it, so re-running an export never destroys the
/// previous one.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_bytes_as_file(bytes: &[u8], filename: &str) -> Result<Option<String>, String> {
    let dir = dirs::download_dir()
        .or_else(dirs::document_dir)
        .or_else(dirs::home_dir)
        .ok_or_else(|| "could not find a directory to save into".to_string())?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let path = unique_path(&dir, filename);
    std::fs::write(&path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

/// A browser tab opened before the PDF bytes exist, so the pop-up blocker
/// sees it as a direct result of the click (MAPPS-1005). The desktop build
/// has no such tab; its `PreviewTab` is a marker that [`show_bytes_in_tab`]
/// ignores in favor of the system PDF viewer.
#[cfg(target_arch = "wasm32")]
pub struct PreviewTab(web_sys::Window);
#[cfg(not(target_arch = "wasm32"))]
pub struct PreviewTab;

/// Open a blank tab synchronously, before the PDF is fetched. `Err` means
/// the browser blocked the tab (or none opened); the caller falls back to
/// downloading rather than leaving the click with no visible result.
#[cfg(target_arch = "wasm32")]
pub fn open_preview_tab() -> Result<PreviewTab, String> {
    let window = web_sys::window().ok_or_else(|| "no window available".to_string())?;
    let tab = window
        .open_with_url_and_target("", "_blank")
        .map_err(|_| "could not open a new tab".to_string())?
        .ok_or_else(|| "the browser blocked the new tab".to_string())?;
    Ok(PreviewTab(tab))
}

/// The desktop build opens the system PDF viewer once the bytes are in,
/// instead of pre-opening a tab, so this always succeeds.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_preview_tab() -> Result<PreviewTab, String> {
    Ok(PreviewTab)
}

/// Show `bytes` as a PDF: in the tab [`open_preview_tab`] opened on the
/// browser, or in the system's PDF viewer on the desktop.
#[cfg(target_arch = "wasm32")]
pub fn show_bytes_in_tab(tab: PreviewTab, bytes: &[u8]) -> Result<(), String> {
    use wasm_bindgen::{closure::Closure, JsCast};

    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array);
    let blob_opts = web_sys::BlobPropertyBag::new();
    blob_opts.set_type("application/pdf");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &blob_opts)
        .map_err(|_| "could not build the preview blob".to_string())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| "could not create the preview URL".to_string())?;

    let window = tab.0;
    window
        .location()
        .set_href(&url)
        .map_err(|_| "could not show the PDF in the new tab".to_string())?;

    // Revoked on the tab's `load`, not right away: revoking before the
    // navigation finishes can leave the tab blank.
    let revoke_url = url.clone();
    let revoke = Closure::once(move || {
        let _ = web_sys::Url::revoke_object_url(&revoke_url);
    });
    let _ = window.add_event_listener_with_callback("load", revoke.as_ref().unchecked_ref());
    revoke.forget();
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn show_bytes_in_tab(_tab: PreviewTab, bytes: &[u8]) -> Result<(), String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("mokosh-preview-{nonce}.pdf"));
    std::fs::write(&path, bytes).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    open::that_detached(&path).map_err(|e| format!("could not open the system PDF viewer: {e}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn unique_path(dir: &std::path::Path, filename: &str) -> std::path::PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let path = std::path::Path::new(filename);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.to_string());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    // Bounded so a directory that somehow rejects every name cannot
    // spin here; past the cap the last candidate is returned and the
    // write reports whatever the filesystem says about it.
    for n in 1..1000 {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem} (1000){ext}"))
}

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

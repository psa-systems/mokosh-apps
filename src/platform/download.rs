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
pub async fn save_bytes_as_file(bytes: &[u8], filename: &str) -> Result<Option<String>, String> {
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
pub async fn save_bytes_as_file(bytes: &[u8], filename: &str) -> Result<Option<String>, String> {
    save_bytes_into(resolved_download_dir()?, bytes, filename).await
}

/// Where [`save_bytes_as_file`] puts a file: the per-user downloads
/// directory, else documents, else home.
///
/// Split out so the write below can be driven against a directory chosen by
/// the caller (MAPPS-1073). The test for the traversal rule used to call
/// `save_bytes_as_file` itself, which meant it wrote into whichever of these
/// three the machine actually had: a developer's real Downloads folder, and
/// inside `just pre-push`'s container, `/`, because that container runs as a
/// uid with no passwd entry and no `HOME`, so all three resolvers fall
/// through to the root. The test failed there on every clean checkout
/// (`could not write /evil: Permission denied`), which is tracked upstream as
/// PC-94; a unit test should not have been reaching for a real user directory
/// either way.
#[cfg(not(target_arch = "wasm32"))]
fn resolved_download_dir() -> Result<std::path::PathBuf, String> {
    dirs::download_dir()
        .or_else(dirs::document_dir)
        .or_else(dirs::home_dir)
        .ok_or_else(|| "could not find a directory to save into".to_string())
}

/// Write `bytes` into `dir` as `filename`, never outside `dir`.
///
/// The containment is the property worth stating: `filename` arrives from a
/// server-sent `Content-Disposition` or a caller-built name, so it goes
/// through `sanitize_filename` (MAPPS-1046) before any `join`, and
/// `unique_path` then picks a non-colliding name inside the same directory.
#[cfg(not(target_arch = "wasm32"))]
async fn save_bytes_into(
    dir: std::path::PathBuf,
    bytes: &[u8],
    filename: &str,
) -> Result<Option<String>, String> {
    let filename = filename.to_string();
    let bytes = bytes.to_vec();
    // The blocking `std::fs` calls run on tokio's blocking pool, not the
    // async worker pool, so a slow disk does not stall other in-flight
    // async UI work sharing that pool.
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("could not create {}: {e}", dir.display()))?;

        let filename = sanitize_filename(&filename);
        let path = unique_path(&dir, &filename);
        std::fs::write(&path, &bytes)
            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| format!("the save task panicked: {e}"))?
}

/// Keep only the final path component of `filename`, discarding any `..`
/// or absolute-path prefix, so it is always safe to `dir.join` directly.
#[cfg(not(target_arch = "wasm32"))]
fn sanitize_filename(filename: &str) -> String {
    std::path::Path::new(filename)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string())
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

#[cfg(target_arch = "wasm32")]
impl PreviewTab {
    /// Close a tab [`open_preview_tab`] opened but that will never be
    /// filled: the fetch behind it failed, so leaving it open would leave
    /// the user with a blank tab and no indication it will never populate.
    pub fn close(self) {
        let _ = self.0.close();
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PreviewTab {
    /// The desktop build never pre-opens anything, so there is nothing to
    /// close.
    pub fn close(self) {}
}

/// Show `bytes` as a PDF: in the tab [`open_preview_tab`] opened on the
/// browser, or in the system's PDF viewer on the desktop.
#[cfg(target_arch = "wasm32")]
pub async fn show_bytes_in_tab(tab: PreviewTab, bytes: &[u8]) -> Result<(), String> {
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
pub async fn show_bytes_in_tab(_tab: PreviewTab, bytes: &[u8]) -> Result<(), String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let bytes = bytes.to_vec();
    // See `save_bytes_as_file`: the write (and the viewer launch) run on
    // tokio's blocking pool rather than the async worker pool.
    tokio::task::spawn_blocking(move || {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("mokosh-preview-{nonce}.pdf"));
        std::fs::write(&path, &bytes)
            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
        open::that_detached(&path).map_err(|e| format!("could not open the system PDF viewer: {e}"))
    })
    .await
    .map_err(|e| format!("the preview task panicked: {e}"))?
}

#[cfg(not(target_arch = "wasm32"))]
fn unique_path(dir: &std::path::Path, filename: &str) -> std::path::PathBuf {
    let filename = sanitize_filename(filename);
    let candidate = dir.join(&filename);
    if !candidate.exists() {
        return candidate;
    }
    let path = std::path::Path::new(&filename);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.clone());
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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_strips_traversal_and_absolute_paths() {
        assert_eq!(sanitize_filename("../../evil"), "evil");
        assert_eq!(sanitize_filename("/etc/evil"), "evil");
        assert_eq!(sanitize_filename("report.pdf"), "report.pdf");
    }

    #[test]
    fn unique_path_never_escapes_dir() {
        let dir = std::env::temp_dir().join("mokosh-apps-unique-path-test");
        std::fs::create_dir_all(&dir).unwrap();

        for filename in ["../../evil", "/etc/evil"] {
            let candidate = unique_path(&dir, filename);
            assert_eq!(candidate.parent(), Some(dir.as_path()));
        }
    }

    /// MAPPS-1073: the same property MAPPS-1046 added, asserted against a
    /// directory this test owns.
    ///
    /// It used to call `save_bytes_as_file`, which resolves
    /// downloads-else-documents-else-home, so the assertion ran against
    /// whatever real directory the machine had: a developer's actual Downloads
    /// folder, and `/` inside `just pre-push`'s container, where it failed on
    /// every clean checkout because that container runs as a uid with no
    /// `HOME` (PC-94). Driving `save_bytes_into` states the containment
    /// without depending on the environment having a writable per-user
    /// directory, or on the test cleaning up after itself in someone's home.
    #[tokio::test]
    async fn save_bytes_into_never_escapes_the_directory_it_was_given() {
        let dir = std::env::temp_dir().join(format!(
            "mokosh-apps-save-bytes-test-{}",
            uuid::Uuid::new_v4().simple()
        ));

        for filename in ["../../evil", "/etc/evil"] {
            let saved = save_bytes_into(dir.clone(), b"test", filename)
                .await
                .expect("save should succeed");
            let saved_path = std::path::PathBuf::from(saved.expect("desktop build returns a path"));
            assert_eq!(
                saved_path.parent(),
                Some(dir.as_path()),
                "{filename} escaped the directory it was given"
            );
            assert_eq!(
                std::fs::read(&saved_path).expect("the bytes landed"),
                b"test",
                "the write has to actually happen, or containment is trivially true"
            );
        }

        // A second save of the same name goes beside the first, not over it
        // (`unique_path`), and still inside the directory.
        let again = save_bytes_into(dir.clone(), b"second", "../../evil")
            .await
            .expect("save should succeed")
            .expect("desktop build returns a path");
        let again = std::path::PathBuf::from(again);
        assert_eq!(again.parent(), Some(dir.as_path()));
        assert_ne!(
            again.file_name(),
            Some(std::ffi::OsStr::new("evil")),
            "the first file must not be overwritten"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The directory the real entry point writes into is still the
    /// downloads-else-documents-else-home chain, and resolving it writes
    /// nothing.
    ///
    /// Asserted without a filesystem write, which is the point: in an
    /// environment with no per-user directory at all this reports the error
    /// instead of attempting `/`.
    #[test]
    fn the_resolved_directory_is_absolute_or_an_error() {
        match resolved_download_dir() {
            Ok(dir) => assert!(dir.is_absolute(), "{} is not absolute", dir.display()),
            Err(e) => assert!(
                e.contains("could not find a directory to save into"),
                "unexpected error: {e}"
            ),
        }
    }
}

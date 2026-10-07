//! Client-side "save bytes as a file" helper (MAPPS-364).
//!
//! The SPA holds the bearer token in memory, so an attachment endpoint
//! cannot be reached with a plain `<a href>` navigation (it carries no
//! Authorization header). The page fetches the bytes with the bearer via
//! [`crate::hooks::fetch::api::get_authed_bytes`] and hands them here.
//!
//! MAPPS-504: the saving itself is [`crate::platform::download`]. In the
//! browser it is a Blob behind a synthesized anchor and the browser
//! reports where the file went; on the desktop this code picks the
//! destination and returns it, so the caller must show the path.

/// Save `bytes` to the user's machine as `filename`.
///
/// `Ok(None)`: the host told the user where it went. `Ok(Some(path))`:
/// it did not, and `path` is where the file is.
pub fn save_bytes_as_file(bytes: &[u8], filename: &str) -> Result<Option<String>, String> {
    crate::platform::download::save_bytes_as_file(bytes, filename)
}

/// A tab opened before the preview PDF is fetched (MAPPS-1005). Open it
/// with [`open_preview_tab`] synchronously in the click handler, then hand
/// it to [`show_bytes_in_tab`] once the bytes arrive.
pub type PreviewTab = crate::platform::download::PreviewTab;

/// Open a blank tab (or, on the desktop build, a no-op placeholder) before
/// the PDF is fetched, so a browser's pop-up blocker sees it as a direct
/// result of the click rather than of an async callback. `Err` means the
/// tab could not be opened; the caller should fall back to downloading.
pub fn open_preview_tab() -> Result<PreviewTab, String> {
    crate::platform::download::open_preview_tab()
}

/// Show `bytes` as a PDF in `tab`: the browser displays it in place, with
/// no file saved; the desktop build opens it in the system's PDF viewer.
pub fn show_bytes_in_tab(tab: PreviewTab, bytes: &[u8]) -> Result<(), String> {
    crate::platform::download::show_bytes_in_tab(tab, bytes)
}

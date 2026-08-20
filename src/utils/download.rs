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

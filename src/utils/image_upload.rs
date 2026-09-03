//! Client-side checks for a KB image upload (MAPPS-587).
//!
//! These mirror the server's rules in `src/modules/knowledge_base/attachments.rs`
//! so a rejection is not the first feedback the author gets. They are a
//! courtesy, never the gate: the server re-checks every upload and its cap is
//! operator-tunable (`KB_ATTACHMENT_MAX_BYTES`), so a file this module accepts
//! can still come back refused and that refusal has to be shown rather than
//! swallowed.
//!
//! SVG is absent from the list on purpose, and for the same reason it is absent
//! server-side: an SVG is a document that can carry script, and these bytes are
//! served from an unauthenticated URL that any browser will render.

/// What the server accepts. Kept in the same order as `ALLOWED_MIME` there.
pub const ALLOWED_MIME: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];

/// The server's default cap. It can be raised by an operator, so a file over
/// this is refused locally only because refusing it here is faster and clearer
/// than a 400 after uploading five megabytes.
pub const MAX_BYTES: usize = 5 * 1024 * 1024;

/// The `accept` attribute for a file input, so the picker offers the same set
/// this module enforces rather than a list that has to be kept in step by hand.
pub fn accept_attribute() -> String {
    ALLOWED_MIME.join(",")
}

/// `Ok(())` when the file is worth sending, otherwise the message to show the
/// author. Phrased for a person: what was wrong and what would work.
pub fn check(mime: &str, len: usize) -> Result<(), String> {
    // A browser reports no type for a file it does not recognise, and some
    // report `application/octet-stream` for a drag from an unusual source.
    // Either way the server would refuse it, so say so here.
    if !ALLOWED_MIME.contains(&mime) {
        return Err(format!(
            "That file is a {}. Images have to be PNG, JPEG, WebP or GIF.",
            if mime.is_empty() {
                "unknown type"
            } else {
                mime
            }
        ));
    }
    if len > MAX_BYTES {
        return Err(format!(
            "That image is {}. The limit is {}.",
            human_size(len),
            human_size(MAX_BYTES)
        ));
    }
    if len == 0 {
        return Err("That file is empty.".to_string());
    }
    Ok(())
}

/// Alt text to start from, derived from the file name: the stem, with the
/// separators people actually use turned back into spaces.
///
/// Not a caption and not a guess at content. It exists so the inserted Markdown
/// is never `![](url)`, which reads as nothing at all to a screen reader, and
/// so the author has something to edit rather than something to write.
pub fn alt_from_file_name(file_name: &str) -> String {
    let stem = file_name
        .rsplit_once('.')
        .map(|(before, _)| before)
        .unwrap_or(file_name);
    let cleaned: String = stem
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "Image".to_string()
    } else {
        collapsed
    }
}

fn human_size(bytes: usize) -> String {
    const MIB: usize = 1024 * 1024;
    const KIB: usize = 1024;
    if bytes >= MIB {
        let whole = bytes as f64 / MIB as f64;
        format!("{whole:.1} MB")
    } else if bytes >= KIB {
        format!("{} KB", bytes / KIB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_allow_list_matches_the_server() {
        // If these drift, an author is told a file is fine and then the upload
        // 400s, or is refused a file the server would have taken.
        assert_eq!(
            ALLOWED_MIME,
            &["image/png", "image/jpeg", "image/webp", "image/gif"]
        );
        assert!(
            !ALLOWED_MIME.contains(&"image/svg+xml"),
            "SVG can carry script and these bytes are served unauthenticated"
        );
    }

    #[test]
    fn a_wrong_type_is_refused_by_name() {
        let err = check("application/pdf", 10).unwrap_err();
        assert!(err.contains("application/pdf"), "{err}");
        assert!(err.contains("PNG"), "and says what would work: {err}");
    }

    #[test]
    fn an_unknown_type_still_reads_as_a_sentence() {
        let err = check("", 10).unwrap_err();
        assert!(err.contains("unknown type"), "{err}");
        assert!(
            !err.contains("a ."),
            "no empty gap where the type goes: {err}"
        );
    }

    #[test]
    fn an_oversized_image_names_both_numbers() {
        let err = check("image/png", MAX_BYTES + 1).unwrap_err();
        assert!(err.contains("5.0 MB"), "the limit: {err}");
    }

    #[test]
    fn an_empty_file_is_refused() {
        assert!(check("image/png", 0).is_err());
    }

    #[test]
    fn a_normal_image_passes() {
        assert!(check("image/png", 1024).is_ok());
        assert!(check("image/jpeg", MAX_BYTES).is_ok());
    }

    #[test]
    fn alt_text_comes_from_the_file_name() {
        assert_eq!(alt_from_file_name("network-diagram.png"), "network diagram");
        assert_eq!(
            alt_from_file_name("Screen_Shot_2026.jpg"),
            "Screen Shot 2026"
        );
        assert_eq!(alt_from_file_name("no-extension"), "no extension");
    }

    /// A name that reduces to nothing still has to produce alt text: `![](url)`
    /// is announced as nothing at all.
    #[test]
    fn alt_text_is_never_empty() {
        assert_eq!(alt_from_file_name(".png"), "Image");
        assert_eq!(alt_from_file_name("___.png"), "Image");
        assert_eq!(alt_from_file_name(""), "Image");
    }

    #[test]
    fn the_accept_attribute_is_the_same_list() {
        let accept = accept_attribute();
        for mime in ALLOWED_MIME {
            assert!(accept.contains(mime), "{mime} missing from {accept}");
        }
    }
}

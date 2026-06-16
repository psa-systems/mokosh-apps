//! Render KB article Markdown to sanitized HTML for direct injection
//! via Dioxus `dangerous_inner_html`. Authors are internal staff, but
//! the same content feeds the public portal feed, so the HTML is always
//! scrubbed with ammonia before it reaches a browser.

use pulldown_cmark::{html, Options, Parser};

/// Markdown -> raw (unsanitized) HTML, with the GFM extensions we render.
fn to_html(src: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(src, options);
    let mut html_out = String::new();
    html::push_html(&mut html_out, parser);
    html_out
}

/// Scrub raw HTML with ammonia, additionally allowing the task-list checkbox
/// `<input>` that `ENABLE_TASKLISTS` emits (the default allowlist drops it,
/// leaving the checkbox text bare - PMS-347). `type` is permitted only via
/// `add_tag_attribute_values` (restricted to `checkbox`); also listing it in
/// `add_tag_attributes` would let the generic allowlist win and pass any value
/// (e.g. `type="text"`). `data-ti` carries the task index for interactive
/// toggling (PMS-348).
fn sanitize(html: &str) -> String {
    ammonia::Builder::default()
        .add_tags(["input"])
        .add_tag_attributes("input", ["checked", "disabled", "data-ti"])
        .add_tag_attribute_values("input", "type", ["checkbox"])
        .clean(html)
        .to_string()
}

/// Render Markdown source to sanitized HTML. Task-list checkboxes render
/// `disabled` (read-only display).
pub fn render_markdown(src: &str) -> String {
    sanitize(&to_html(src))
}

/// Like [`render_markdown`] but task-list checkboxes are interactive
/// (PMS-348): the `disabled` attribute is stripped and each checkbox is
/// tagged with `data-ti="<index>"` (its 0-based order in the document) so a
/// click handler can map the click back to a task item. Used only where the
/// host wires an `on_toggle` callback.
pub fn render_markdown_interactive(src: &str) -> String {
    // pulldown emits each task checkbox as `<input disabled="" type="checkbox"`
    // (optionally ` checked=""`). Drop `disabled` to make them clickable, then
    // tag each in document order with its index.
    let html = to_html(src).replace(" disabled=\"\"", "");
    let needle = "<input type=\"checkbox\"";
    let mut out = String::with_capacity(html.len());
    let mut rest = html.as_str();
    let mut i = 0usize;
    while let Some(pos) = rest.find(needle) {
        out.push_str(&rest[..pos]);
        out.push_str(&format!("<input data-ti=\"{i}\" type=\"checkbox\""));
        i += 1;
        rest = &rest[pos + needle.len()..];
    }
    out.push_str(rest);
    sanitize(&out)
}

/// Toggle the `index`-th GFM task-list item in `src` (`- [ ]` <-> `- [x]`),
/// counting markers in document order to match the rendered checkbox order
/// (PMS-348). Returns the updated source, or `None` if `index` is out of
/// range. Only the bracket marker flips; the rest of the line is untouched.
pub fn toggle_task(src: &str, index: usize) -> Option<String> {
    let mut seen = 0usize;
    let mut out: Vec<String> = Vec::new();
    let mut toggled = false;
    for line in src.split_inclusive('\n') {
        // Split off a trailing newline so we can edit the content then re-add.
        let (content, nl) = match line.strip_suffix('\n') {
            Some(c) => (c, "\n"),
            None => (line, ""),
        };
        if let Some(new_content) = toggle_task_line(content, seen, index) {
            out.push(format!("{new_content}{nl}"));
            toggled = true;
            seen += 1;
        } else if is_task_line(content) {
            // A task line that is not the target: keep it, advance the count.
            out.push(line.to_string());
            seen += 1;
        } else {
            out.push(line.to_string());
        }
    }
    toggled.then(|| out.concat())
}

/// If `content` is a task-list line and `seen == target`, return the line with
/// its marker flipped; otherwise `None`.
fn toggle_task_line(content: &str, seen: usize, target: usize) -> Option<String> {
    let pos = task_marker_pos(content)?;
    if seen != target {
        return None;
    }
    let bytes = content.as_bytes();
    let new_char = if bytes[pos] == b' ' { 'x' } else { ' ' };
    let mut s = content.to_string();
    s.replace_range(pos..pos + 1, &new_char.to_string());
    Some(s)
}

fn is_task_line(content: &str) -> bool {
    task_marker_pos(content).is_some()
}

/// Byte offset of the marker char inside a GFM task line's `[ ]` / `[x]`, i.e.
/// the position of the space/x. A task line is `<indent>[-*+] [<marker>] ...`.
fn task_marker_pos(content: &str) -> Option<usize> {
    let trimmed_start = content.len() - content.trim_start().len();
    let rest = &content[trimmed_start..];
    let bytes = rest.as_bytes();
    // bullet + at least one space + "[x]" or "[ ]"
    if bytes.len() < 4 || !matches!(bytes[0], b'-' | b'*' | b'+') || bytes[1] != b' ' {
        return None;
    }
    // skip the bullet and following spaces
    let mut i = 1;
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    if i + 2 < bytes.len()
        && bytes[i] == b'['
        && matches!(bytes[i + 1], b' ' | b'x' | b'X')
        && bytes[i + 2] == b']'
    {
        Some(trimmed_start + i + 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_headings_and_lists() {
        let out = render_markdown("# Title\n\n- a\n- b");
        assert!(out.contains("<h1>Title</h1>"));
        assert!(out.contains("<li>a</li>"));
    }

    #[test]
    fn renders_code_block() {
        let out = render_markdown("```\nlet x = 1;\n```");
        assert!(out.contains("<pre><code"));
        assert!(out.contains("let x = 1;"));
    }

    #[test]
    fn strips_script_tags() {
        let out = render_markdown("hello <script>alert(1)</script> world");
        assert!(!out.contains("<script"));
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
    }

    #[test]
    fn strips_event_handler_attributes() {
        let out = render_markdown("<img src=x onerror=\"alert(1)\">");
        assert!(!out.contains("onerror"));
    }

    #[test]
    fn keeps_links_but_drops_javascript_scheme() {
        let out = render_markdown("[x](javascript:alert(1))");
        assert!(!out.contains("javascript:"));
    }

    #[test]
    fn renders_task_list_checkboxes() {
        let out = render_markdown("- [ ] todo\n- [x] done");
        // Both items keep a checkbox input...
        assert_eq!(out.matches("type=\"checkbox\"").count(), 2);
        // ...the `[x]` one is checked, and they stay disabled (read-only).
        assert!(out.contains("checked"));
        assert!(out.contains("disabled"));
        assert!(out.contains("todo"));
        assert!(out.contains("done"));
    }

    #[test]
    fn does_not_allow_non_checkbox_inputs() {
        let out = render_markdown("<input type=\"text\" value=\"x\">");
        assert!(!out.contains("type=\"text\""));
    }

    #[test]
    fn interactive_checkboxes_enabled_and_indexed() {
        let out = render_markdown_interactive("- [ ] a\n- [x] b\n- [ ] c");
        // No longer disabled, and each checkbox carries its document index.
        assert!(!out.contains("disabled"));
        assert!(out.contains("data-ti=\"0\""));
        assert!(out.contains("data-ti=\"1\""));
        assert!(out.contains("data-ti=\"2\""));
        assert_eq!(out.matches("type=\"checkbox\"").count(), 3);
        // The `[x]` one is still rendered checked.
        assert!(out.contains("checked"));
    }

    #[test]
    fn toggle_task_flips_the_indexed_marker() {
        let src = "- [ ] a\n- [x] b\n- [ ] c";
        assert_eq!(toggle_task(src, 0).unwrap(), "- [x] a\n- [x] b\n- [ ] c");
        assert_eq!(toggle_task(src, 1).unwrap(), "- [ ] a\n- [ ] b\n- [ ] c");
        assert_eq!(toggle_task(src, 2).unwrap(), "- [ ] a\n- [x] b\n- [x] c");
    }

    #[test]
    fn toggle_task_preserves_surrounding_text_and_indent() {
        let src = "# Plan\n\n- [ ] top\n  - [ ] nested\n\nfooter";
        // index 1 is the nested item; only its marker flips.
        assert_eq!(
            toggle_task(src, 1).unwrap(),
            "# Plan\n\n- [ ] top\n  - [x] nested\n\nfooter"
        );
        // `*` and `+` bullets count as task lines too.
        assert_eq!(toggle_task("* [ ] a", 0).unwrap(), "* [x] a");
        assert_eq!(toggle_task("+ [x] a", 0).unwrap(), "+ [ ] a");
    }

    #[test]
    fn toggle_task_out_of_range_is_none() {
        assert!(toggle_task("- [ ] only", 1).is_none());
        assert!(toggle_task("no tasks here", 0).is_none());
    }
}

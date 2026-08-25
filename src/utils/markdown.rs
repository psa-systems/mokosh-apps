//! Render KB article Markdown to sanitized HTML for direct injection
//! via Dioxus `dangerous_inner_html`. Authors are internal staff, but
//! the same content feeds the public portal feed, so the HTML is always
//! scrubbed with ammonia before it reaches a browser.

use pulldown_cmark::{html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::utils::highlight;

/// Markdown -> raw (unsanitized) HTML, with the GFM extensions we render.
///
/// The event stream is transformed rather than post-processed on the HTML,
/// because both passes here need to know WHERE they are: highlighting must see
/// only fenced-code text, and autolinking must see only text that is not
/// already inside a link or a code span. A regex over the finished HTML cannot
/// tell those apart and would happily rewrite the inside of an `href`.
fn to_html(src: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(src, options);

    let mut events: Vec<Event> = Vec::new();
    // Fence info string while inside a fenced block; `None` outside one.
    let mut in_code: Option<String> = None;
    // Depth of enclosing links, so a bare URL used as a link's own text is not
    // turned into a link inside a link.
    let mut link_depth = 0usize;

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(ref kind)) => {
                in_code = Some(match kind {
                    CodeBlockKind::Fenced(info) => info.to_string(),
                    CodeBlockKind::Indented => String::new(),
                });
                events.push(event);
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = None;
                events.push(event);
            }
            Event::Text(ref text) if in_code.is_some() => {
                let lang = in_code.as_deref().unwrap_or("");
                // Already-escaped HTML, so it goes back as `Html`, not `Text`:
                // pushing it as text would escape the spans we just added.
                events.push(Event::Html(highlight::highlight_html(lang, text).into()));
            }
            Event::Start(Tag::Link { .. }) => {
                link_depth += 1;
                events.push(event);
            }
            Event::End(TagEnd::Link) => {
                link_depth = link_depth.saturating_sub(1);
                events.push(event);
            }
            Event::Text(ref text) if link_depth == 0 => {
                autolink_into(text, &mut events);
            }
            other => events.push(other),
        }
    }

    let mut html_out = String::new();
    html::push_html(&mut html_out, events.into_iter());
    html_out
}

/// Split `text` into plain runs and bare URLs, pushing link events for the
/// URLs (MAPPS-573).
///
/// pulldown-cmark 0.12 has no GFM autolink option (`ENABLE_GFM` covers
/// blockquote alerts, not literal links), so a pasted URL rendered as plain
/// text while `[label](url)` worked. Authors paste URLs.
fn autolink_into<'a>(text: &str, events: &mut Vec<Event<'a>>) {
    let mut rest = text;
    let mut plain_start = 0usize;
    let mut pushed_any = false;

    while let Some(found) = find_url(&rest[plain_start..]) {
        let start = plain_start + found.start;
        let end = plain_start + found.end;
        if start > 0 {
            events.push(Event::Text(rest[..start].to_string().into()));
        }
        let url = rest[start..end].to_string();
        events.push(Event::Start(Tag::Link {
            link_type: pulldown_cmark::LinkType::Autolink,
            dest_url: url.clone().into(),
            title: "".into(),
            id: "".into(),
        }));
        events.push(Event::Text(url.into()));
        events.push(Event::End(TagEnd::Link));
        pushed_any = true;
        rest = &rest[end..];
        plain_start = 0;
    }

    if pushed_any {
        if !rest.is_empty() {
            events.push(Event::Text(rest.to_string().into()));
        }
    } else {
        events.push(Event::Text(text.to_string().into()));
    }
}

struct Found {
    start: usize,
    end: usize,
}

/// Locate the first bare `http://` or `https://` URL in `s`.
///
/// Only those two schemes: `mailto:` and friends are not what authors paste,
/// and a permissive scheme match is how an autolinker starts turning `C:\path`
/// and `note:see below` into links. The end is trimmed of trailing punctuation
/// so a URL at the end of a sentence does not swallow the full stop, and of an
/// unbalanced closing paren so a URL inside `(see https://x)` stays intact.
fn find_url(s: &str) -> Option<Found> {
    let start = ["https://", "http://"]
        .iter()
        .filter_map(|scheme| s.find(scheme))
        .min()?;
    // A scheme glued to the end of a word (`xhttps://y`) is not a link.
    if start > 0 {
        let prev = s[..start].chars().next_back().unwrap_or(' ');
        if prev.is_alphanumeric() {
            return None;
        }
    }
    let mut end = s[start..]
        .find(|c: char| c.is_whitespace() || c == '<' || c == '"')
        .map(|p| start + p)
        .unwrap_or(s.len());
    let body = &s[start..end];
    // Nothing after the scheme is not a URL.
    let scheme_len = if body.starts_with("https://") { 8 } else { 7 };
    if body.len() <= scheme_len {
        return None;
    }
    let mut trimmed = body;
    while let Some(last) = trimmed.chars().next_back() {
        let drop = matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*' | '_')
            || (last == ')' && trimmed.matches(')').count() > trimmed.matches('(').count());
        if !drop {
            break;
        }
        trimmed = &trimmed[..trimmed.len() - last.len_utf8()];
    }
    if trimmed.len() <= scheme_len {
        return None;
    }
    end = start + trimmed.len();
    Some(Found { start, end })
}

/// Scrub raw HTML with ammonia, additionally allowing the task-list checkbox
/// `<input>` that `ENABLE_TASKLISTS` emits (the default allowlist drops it,
/// leaving the checkbox text bare - PMS-347). `type` is permitted only via
/// `add_tag_attribute_values` (restricted to `checkbox`); also listing it in
/// `add_tag_attributes` would let the generic allowlist win and pass any value
/// (e.g. `type="text"`). `data-ti` carries the task index for interactive
/// toggling (PMS-348).
fn sanitize(html: &str) -> String {
    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(["input"])
        .add_tag_attributes("input", ["checked", "disabled", "data-ti"])
        .add_tag_attribute_values("input", "type", ["checkbox"])
        // MAPPS-573: the highlighter's token classes. `add_allowed_classes`
        // permits these VALUES on `class` and nothing else, so an author who
        // writes raw `<span class="...">` in an article cannot reach for an
        // arbitrary app style; the tag also has to be one we already allow.
        .add_allowed_classes("span", highlight::CLASSES.iter().copied())
        // The language marker pulldown puts on a fenced block. Kept for the
        // block styling and so a reader can see what the fence claimed, even
        // though the colours come from the spans inside.
        .add_allowed_classes("code", LANGUAGE_CLASSES.iter().copied())
        .add_tag_attributes("span", ["style"])
        // MAPPS-573: authors colour text with `<span style="color:red">`, which
        // the default allowlist dropped entirely. Blanket `style` is a CSS
        // injection surface and this content also feeds the public portal, so
        // the attribute is permitted and then filtered down to a single
        // property with a value shape that cannot carry a URL, an expression or
        // a second declaration. Anything else is dropped, not sanitized in
        // place, so a rejected style leaves no half-applied rule behind.
        .attribute_filter(|_tag, attr, value| {
            if attr != "style" {
                return Some(value.into());
            }
            safe_color_style(value).map(Into::into)
        });
    builder.clean(html).to_string()
}

/// `class` values permitted on a fenced code block: `language-<name>` for the
/// languages the highlighter knows, plus the plain marker pulldown emits.
static LANGUAGE_CLASSES: &[&str] = &[
    "language-bash",
    "language-console",
    "language-diff",
    "language-ini",
    "language-js",
    "language-json",
    "language-jsonc",
    "language-patch",
    "language-python",
    "language-rust",
    "language-shell",
    "language-sql",
    "language-toml",
    "language-ts",
    "language-typescript",
    "language-yaml",
    "language-yml",
];

/// Accept `color: <value>` and nothing else.
///
/// Returns the normalized declaration, or `None` to drop the attribute. The
/// value must be a bare CSS named colour (letters only) or a `#rgb`/`#rrggbb`
/// hex literal. That shape cannot express `url(...)`, an `expression(...)`, a
/// escaped sequence, or a second property smuggled in after a `;`, so the
/// filter does not need to know what those look like: everything outside the
/// shape is rejected rather than scrubbed.
fn safe_color_style(value: &str) -> Option<String> {
    let decl = value.trim().trim_end_matches(';').trim();
    let (prop, raw) = decl.split_once(':')?;
    if !prop.trim().eq_ignore_ascii_case("color") {
        return None;
    }
    let color = raw.trim();
    if color.is_empty() || color.len() > 32 {
        return None;
    }
    let ok = if let Some(hex) = color.strip_prefix('#') {
        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
    } else {
        color.chars().all(|c| c.is_ascii_alphabetic())
    };
    ok.then(|| format!("color:{color}"))
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

    // MAPPS-573 -------------------------------------------------------------

    /// A fenced block keeps its language marker and carries token spans. Before
    /// this, `ammonia::Builder::default()` stripped `class`, so pulldown's
    /// `language-rust` never reached the DOM and no CSS-based highlighting of
    /// any kind could have worked.
    #[test]
    fn fenced_code_is_highlighted_and_keeps_its_language() {
        let out = render_markdown("```rust\nfn main() { let s = \"hi\"; }\n```");
        assert!(
            out.contains(r#"<code class="language-rust""#),
            "the fence language must survive sanitizing: {out}"
        );
        assert!(
            out.contains(r#"<span class="hl-kw">fn</span>"#),
            "token spans must survive sanitizing: {out}"
        );
        // ammonia re-serializes, so a quote inside text comes back as a raw
        // `"` rather than the `&quot;` the highlighter wrote. Either is correct
        // HTML; assert on the span, not on the entity.
        assert!(out.contains(r#"<span class="hl-str">"hi"</span>"#), "{out}");
    }

    /// The allowlist and the highlighter must not drift: a class the
    /// highlighter emits but the sanitizer does not permit renders colourless,
    /// with nothing failing anywhere.
    #[test]
    fn every_highlighter_class_survives_the_sanitizer() {
        for class in crate::utils::highlight::CLASSES {
            let html = format!(r#"<span class="{class}">x</span>"#);
            assert!(
                sanitize(&html).contains(class),
                "`{class}` is emitted by the highlighter but dropped by the sanitizer"
            );
        }
    }

    /// A class the highlighter does NOT emit is still dropped, so allowing
    /// `class` on `span` did not open the door to arbitrary app styles being
    /// reachable from article text.
    #[test]
    fn an_arbitrary_class_is_still_dropped() {
        let out = sanitize(r#"<span class="fixed inset-0 z-50">x</span>"#);
        assert!(!out.contains("inset-0"), "{out}");
        assert!(out.contains('x'));
    }

    /// AC6: the author's colour survives.
    #[test]
    fn a_span_colour_is_kept() {
        let out = render_markdown("<span style=\"color:red\">**REST API**</span>");
        assert!(out.contains(r#"style="color:red""#), "{out}");
        assert!(out.contains("<strong>REST API</strong>"), "{out}");
    }

    /// And the rejections, which is the half worth testing. Each of these
    /// drops the whole declaration rather than keeping a scrubbed part of it.
    #[test]
    fn only_a_plain_colour_is_kept_in_a_style() {
        for bad in [
            // A second property riding along after the one we allow.
            "color:red;background:url(https://x/y)",
            // Not the property we allow.
            "background:red",
            "position:fixed;top:0",
            // Values that are not a bare name or a hex literal.
            "color:url(https://evil/x)",
            "color:expression(alert(1))",
            "color:var(--anything)",
            "color:rgb(1,2,3)",
            "color:red !important",
            "color:\\72 ed",
        ] {
            let out = sanitize(&format!(r#"<span style="{bad}">x</span>"#));
            assert!(
                !out.contains("style="),
                "`{bad}` must be dropped entirely, got: {out}"
            );
            assert!(out.contains('x'), "the text itself is kept: {out}");
        }
    }

    #[test]
    fn a_hex_colour_is_kept_and_a_malformed_one_is_not() {
        assert!(sanitize(r#"<span style="color:#c00">x</span>"#).contains("color:#c00"));
        assert!(sanitize(r#"<span style="color:#abcdef">x</span>"#).contains("color:#abcdef"));
        assert!(!sanitize(r#"<span style="color:#xyz">x</span>"#).contains("style="));
        assert!(!sanitize(r#"<span style="color:#12345">x</span>"#).contains("style="));
    }

    /// AC9: a pasted URL becomes a link. pulldown-cmark 0.12 has no GFM
    /// autolink option, so this is our own pass over the event stream.
    #[test]
    fn a_bare_url_becomes_a_link() {
        let out = render_markdown("See https://example.com/a?b=1 for more");
        assert!(
            out.contains(r#"<a href="https://example.com/a?b=1""#),
            "{out}"
        );
    }

    /// The autolinker must not eat the punctuation around the URL, and must not
    /// touch a URL that is already a link or already code.
    #[test]
    fn autolinking_leaves_punctuation_links_and_code_alone() {
        let out = render_markdown("Go to https://example.com/x. Also (https://e.com/y).");
        assert!(out.contains(r#"href="https://example.com/x""#), "{out}");
        assert!(
            out.contains(r#"href="https://e.com/y""#),
            "the closing paren is not part of the URL: {out}"
        );
        assert!(
            out.contains("</a>. Also"),
            "the full stop stays outside the link: {out}"
        );

        // An existing link keeps its own label rather than gaining a nested one.
        let linked = render_markdown("[label](https://q.com)");
        assert_eq!(linked.matches("<a ").count(), 1, "{linked}");
        assert!(linked.contains(">label</a>"), "{linked}");

        // A URL inside code is code, not a link.
        let code = render_markdown("`https://example.com`");
        assert!(!code.contains("<a "), "{code}");
        let fenced = render_markdown("```\nhttps://example.com\n```");
        assert!(!fenced.contains("<a "), "{fenced}");
    }

    /// A scheme glued onto a word is not a URL, and a scheme with nothing after
    /// it is not either.
    #[test]
    fn near_misses_are_not_autolinked() {
        for src in ["nothttps://x.com", "https://", "http://"] {
            let out = render_markdown(src);
            assert!(!out.contains("<a "), "{src} -> {out}");
        }
    }

    /// Highlighting must not change what the code says. The escaping in the
    /// highlighter and the sanitizer run in sequence, so this pins that the
    /// pair of them leaves the text intact.
    #[test]
    fn highlighting_does_not_alter_the_code_text() {
        let out = render_markdown("```js\nif (a < b && c > d) alert(\"x\");\n```");
        assert!(out.contains("&lt;"), "{out}");
        assert!(out.contains("&amp;&amp;"), "{out}");
        assert!(!out.contains("<script"), "{out}");
        // The angle brackets did not become tags.
        assert!(!out.contains("< b"), "{out}");
    }
}

#[cfg(test)]
mod reported_article {
    use super::*;
    const SRC: &str = r#"# Description

Features

* [ ] <span style="color:red">**REST API**</span> - @niceguyit
    - [ ] Secrets and tokens stored in Infisical
* [x] Ticketing. **Built** (PSA-19).

# Build status

Verified by reading code on `main`: mokosh-server `bfeaf92e`. See https://dev.a8n.run/psa-systems for more.

| Issue | Area | Status |
|---|---|---|
| PSA-19 | Help desk | Mostly built |

```bash
just check   # runs the guards
export TOKEN="$MOKOSH_TOKEN"
```
"#;

    #[test]
    fn the_reported_article_renders_every_fixed_element() {
        let out = render_markdown(SRC);
        assert!(out.contains(r#"style="color:red""#), "colour kept: {out}");
        assert!(out.contains("<table>"), "table renders: {out}");
        assert_eq!(out.matches("type=\"checkbox\"").count(), 3, "{out}");
        assert!(out.contains("<code>main</code>"), "inline code: {out}");
        assert!(
            out.contains(r#"<a href="https://dev.a8n.run/psa-systems""#),
            "bare URL autolinked: {out}"
        );
        assert!(
            out.contains(r#"<span class="hl-com"># runs the guards</span>"#),
            "shell comment highlighted: {out}"
        );
        assert!(
            out.contains(r#"<span class="hl-var">$MOKOSH_TOKEN</span>"#),
            "variable inside the quoted string highlighted: {out}"
        );
        assert!(out.contains(r#"<code class="language-bash""#), "{out}");
    }

    #[test]
    fn the_same_article_toggles_its_checkboxes() {
        let out = render_markdown_interactive(SRC);
        assert!(!out.contains("disabled"), "{out}");
        for i in 0..3 {
            assert!(out.contains(&format!("data-ti=\"{i}\"")), "{out}");
        }
        // And the toggle maps back onto the right source line.
        let flipped = toggle_task(SRC, 1).expect("nested item is index 1");
        assert!(flipped.contains("- [x] Secrets and tokens stored in Infisical"));
        assert!(flipped.contains("* [ ] <span style=\"color:red\">**REST API**</span>"));
    }
}

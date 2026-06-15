//! Render KB article Markdown to sanitized HTML for direct injection
//! via Dioxus `dangerous_inner_html`. Authors are internal staff, but
//! the same content feeds the public portal feed, so the HTML is always
//! scrubbed with ammonia before it reaches a browser.

use pulldown_cmark::{html, Options, Parser};

/// Render Markdown source to sanitized HTML.
pub fn render_markdown(src: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(src, options);
    let mut unsafe_html = String::new();
    html::push_html(&mut unsafe_html, parser);
    // Allow the disabled checkbox `<input>` that ENABLE_TASKLISTS emits for
    // `- [ ]` / `- [x]` items (PMS-347); the default ammonia allowlist drops
    // it, leaving the checkbox text bare. `type` is restricted to `checkbox`
    // so no other input kind can slip through, and the checkboxes stay
    // disabled (read-only display). Everything else keeps the default scrub.
    // `type` is allowed only via add_tag_attribute_values (restricted to
    // `checkbox`); listing it in add_tag_attributes too would allow any value
    // (the generic allowlist wins), letting `type="text"` etc. through.
    ammonia::Builder::default()
        .add_tags(["input"])
        .add_tag_attributes("input", ["checked", "disabled"])
        .add_tag_attribute_values("input", "type", ["checkbox"])
        .clean(&unsafe_html)
        .to_string()
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
}

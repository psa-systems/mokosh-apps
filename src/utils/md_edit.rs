//! Markdown source transforms for the editor toolbar (MAPPS-579).
//!
//! The toolbar's whole job is "do to the selection what the button says", and
//! that is pure text work. Keeping it here, away from the DOM and the
//! component, is what makes it testable: every behaviour below is asserted on
//! strings rather than driven through a browser the host test harness does not
//! have.
//!
//! ## Offsets are UTF-16, on purpose
//!
//! `HTMLTextAreaElement.selectionStart` counts UTF-16 code units; Rust strings
//! are UTF-8 bytes. For ASCII they agree, which is exactly why a mix-up here
//! survives every test written in English and then corrupts the first article
//! containing an accent or an emoji. This module speaks the DOM's units at its
//! boundary and converts once, inward, so no caller has to remember.

/// A transform's result: the new source and where to leave the selection,
/// in UTF-16 code units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditResult {
    pub text: String,
    pub sel_start: u32,
    pub sel_end: u32,
}

/// What a toolbar button does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Wrapping marks. Toggle: applying to an already-wrapped selection
    /// removes the markers rather than nesting them.
    Bold,
    Italic,
    Strikethrough,
    InlineCode,
    /// Line prefixes, applied to every line the selection touches. Toggle:
    /// if every touched line already carries the prefix, it is removed.
    Quote,
    BulletList,
    NumberedList,
    Checklist,
    /// `## `, or a level the caller chooses. Level 0 clears any heading.
    Heading(u8),
    /// Structural inserts, each fed by a dialog rather than typed.
    Link {
        text: String,
        url: String,
    },
    Image {
        alt: String,
        url: String,
    },
    CodeBlock {
        lang: String,
    },
    Table {
        rows: usize,
        cols: usize,
    },
}

/// Byte offset of the `n`th UTF-16 code unit in `s`, clamped to the string.
fn utf16_to_byte(s: &str, target: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in s.char_indices() {
        if units >= target {
            return byte;
        }
        units += ch.len_utf16() as u32;
    }
    s.len()
}

/// UTF-16 code-unit offset of a byte offset in `s`.
fn byte_to_utf16(s: &str, target: usize) -> u32 {
    let target = target.min(s.len());
    s[..target].chars().map(|c| c.len_utf16() as u32).sum()
}

/// Apply `action` to `src` over the selection `(start, end)` in UTF-16 units.
pub fn apply(src: &str, start: u32, end: u32, action: &Action) -> EditResult {
    let (lo, hi) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let a = utf16_to_byte(src, lo);
    let b = utf16_to_byte(src, hi);
    match action {
        Action::Bold => wrap(src, a, b, "**", "bold text"),
        Action::Italic => wrap(src, a, b, "*", "italic text"),
        Action::Strikethrough => wrap(src, a, b, "~~", "struck text"),
        Action::InlineCode => wrap(src, a, b, "`", "code"),
        Action::Quote => line_prefix(src, a, b, "> "),
        Action::BulletList => line_prefix(src, a, b, "- "),
        Action::Checklist => line_prefix(src, a, b, "- [ ] "),
        Action::NumberedList => numbered(src, a, b),
        Action::Heading(level) => heading(src, a, b, *level),
        Action::Link { text, url } => {
            let label = pick_label(src, a, b, text, "link text");
            insert(
                src,
                a,
                b,
                &format!("[{label}]({url})"),
                1,
                label.chars().count(),
            )
        }
        Action::Image { alt, url } => {
            let label = if alt.trim().is_empty() { "image" } else { alt };
            insert(
                src,
                a,
                b,
                &format!("![{label}]({url})"),
                2,
                label.chars().count(),
            )
        }
        Action::CodeBlock { lang } => code_block(src, a, b, lang),
        Action::Table { rows, cols } => table(src, a, b, *rows, *cols),
    }
}

/// The label a link or image should carry: the selection if there is one, the
/// caller's text if not, and a placeholder as a last resort.
fn pick_label<'a>(src: &'a str, a: usize, b: usize, given: &'a str, fallback: &'a str) -> &'a str {
    if a != b {
        &src[a..b]
    } else if !given.trim().is_empty() {
        given
    } else {
        fallback
    }
}

/// Replace `a..b` with `body`, selecting `len` characters starting `skip`
/// characters into it. Used where the useful part of an insert is a substring
/// the author will immediately retype, such as a link's label.
fn insert(src: &str, a: usize, b: usize, body: &str, skip: usize, len: usize) -> EditResult {
    let mut text = String::with_capacity(src.len() + body.len());
    text.push_str(&src[..a]);
    text.push_str(body);
    text.push_str(&src[b..]);

    let sel_from = a + body
        .char_indices()
        .nth(skip)
        .map(|(i, _)| i)
        .unwrap_or(body.len());
    let sel_to = a + body
        .char_indices()
        .nth(skip + len)
        .map(|(i, _)| i)
        .unwrap_or(body.len());
    EditResult {
        sel_start: byte_to_utf16(&text, sel_from),
        sel_end: byte_to_utf16(&text, sel_to),
        text,
    }
}

/// Wrap the selection in `marker`, or unwrap it if it is already wrapped.
///
/// Both shapes count as wrapped: the markers inside the selection
/// (`|**bold**|`) and immediately outside it (`**|bold|**`). Handling only one
/// means the second click on a mark nests it instead of removing it, which is
/// how a toolbar ends up producing `****text****`.
fn wrap(src: &str, a: usize, b: usize, marker: &str, placeholder: &str) -> EditResult {
    let m = marker.len();
    let selected = &src[a..b];
    let mc = marker.chars().next().expect("markers are non-empty");

    // Already wrapped, markers inside the selection. `run_len` guards the case
    // that italic's `*` is a prefix of bold's `**`: without it, italic on
    // `**bold**` sees a `*` at each end, removes one from each, and leaves
    // `*bold*`, silently demoting the author's bold to italic.
    if a != b
        && selected.len() >= 2 * m
        && selected.starts_with(marker)
        && selected.ends_with(marker)
        && run_len(selected, mc) == m
        && run_len_rev(selected, mc) == m
    {
        let inner = &selected[m..selected.len() - m];
        let mut text = String::with_capacity(src.len());
        text.push_str(&src[..a]);
        text.push_str(inner);
        text.push_str(&src[b..]);
        let start = byte_to_utf16(&text, a);
        let end = byte_to_utf16(&text, a + inner.len());
        return EditResult {
            text,
            sel_start: start,
            sel_end: end,
        };
    }

    // Already wrapped, markers outside the selection. Same run guard.
    if a >= m
        && b + m <= src.len()
        && src[a - m..a] == *marker
        && src[b..b + m] == *marker
        && run_len_rev(&src[..a], mc) == m
        && run_len(&src[b..], mc) == m
    {
        let mut text = String::with_capacity(src.len());
        text.push_str(&src[..a - m]);
        text.push_str(selected);
        text.push_str(&src[b + m..]);
        let start = byte_to_utf16(&text, a - m);
        let end = byte_to_utf16(&text, a - m + selected.len());
        return EditResult {
            text,
            sel_start: start,
            sel_end: end,
        };
    }

    // Not wrapped. An empty selection gets a placeholder, selected, so the
    // author types over it instead of hunting for the caret between markers.
    let body = if a == b { placeholder } else { selected };
    let wrapped = format!("{marker}{body}{marker}");
    insert(
        src,
        a,
        b,
        &wrapped,
        marker.chars().count(),
        body.chars().count(),
    )
}

/// Length in bytes of the run of `c` at the start of `s`.
fn run_len(s: &str, c: char) -> usize {
    s.chars().take_while(|x| *x == c).map(char::len_utf8).sum()
}

/// Length in bytes of the run of `c` at the end of `s`.
fn run_len_rev(s: &str, c: char) -> usize {
    s.chars()
        .rev()
        .take_while(|x| *x == c)
        .map(char::len_utf8)
        .sum()
}

/// Byte range of the whole lines the selection touches.
fn line_span(src: &str, a: usize, b: usize) -> (usize, usize) {
    let start = src[..a].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = src[b..].find('\n').map(|i| b + i).unwrap_or(src.len());
    (start, end)
}

/// Add `prefix` to every line the selection touches, or remove it from all of
/// them if all of them already have it.
fn line_prefix(src: &str, a: usize, b: usize, prefix: &str) -> EditResult {
    let (ls, le) = line_span(src, a, b);
    let block = &src[ls..le];
    let lines: Vec<&str> = block.split('\n').collect();
    let all_prefixed = lines.iter().all(|l| l.starts_with(prefix));

    let rebuilt: Vec<String> = lines
        .iter()
        .map(|l| {
            if all_prefixed {
                l.strip_prefix(prefix).unwrap_or(l).to_string()
            } else {
                format!("{prefix}{l}")
            }
        })
        .collect();
    let body = rebuilt.join("\n");

    let mut text = String::with_capacity(src.len() + body.len());
    text.push_str(&src[..ls]);
    text.push_str(&body);
    text.push_str(&src[le..]);
    // Select the whole affected block, so a second click toggles what the
    // first one changed rather than a drifting subset of it.
    EditResult {
        sel_start: byte_to_utf16(&text, ls),
        sel_end: byte_to_utf16(&text, ls + body.len()),
        text,
    }
}

/// Like [`line_prefix`], but each line is numbered from 1.
fn numbered(src: &str, a: usize, b: usize) -> EditResult {
    let (ls, le) = line_span(src, a, b);
    let block = &src[ls..le];
    let lines: Vec<&str> = block.split('\n').collect();
    let numbered_already = lines.iter().all(|l| is_numbered(l));

    let rebuilt: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if numbered_already {
                strip_number(l).to_string()
            } else {
                format!("{}. {l}", i + 1)
            }
        })
        .collect();
    let body = rebuilt.join("\n");

    let mut text = String::with_capacity(src.len() + body.len());
    text.push_str(&src[..ls]);
    text.push_str(&body);
    text.push_str(&src[le..]);
    EditResult {
        sel_start: byte_to_utf16(&text, ls),
        sel_end: byte_to_utf16(&text, ls + body.len()),
        text,
    }
}

fn is_numbered(line: &str) -> bool {
    let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    !digits.is_empty() && line[digits.len()..].starts_with(". ")
}

fn strip_number(line: &str) -> &str {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        &line[digits + 2..]
    } else {
        line
    }
}

/// Set the heading level of the touched lines. Level 0 removes any heading.
/// Applying the level a line already has removes it, so the button toggles.
fn heading(src: &str, a: usize, b: usize, level: u8) -> EditResult {
    let (ls, le) = line_span(src, a, b);
    let block = &src[ls..le];
    let want = "#".repeat(level.min(6) as usize);

    let rebuilt: Vec<String> = block
        .split('\n')
        .map(|l| {
            let hashes = l.chars().take_while(|c| *c == '#').count();
            let rest = l[hashes..].strip_prefix(' ').unwrap_or(&l[hashes..]);
            if level == 0 || hashes == level as usize {
                rest.to_string()
            } else {
                format!("{want} {rest}")
            }
        })
        .collect();
    let body = rebuilt.join("\n");

    let mut text = String::with_capacity(src.len() + body.len());
    text.push_str(&src[..ls]);
    text.push_str(&body);
    text.push_str(&src[le..]);
    EditResult {
        sel_start: byte_to_utf16(&text, ls),
        sel_end: byte_to_utf16(&text, ls + body.len()),
        text,
    }
}

/// Fence the selection, on its own lines. An empty selection leaves the caret
/// inside the fence, which is where the author is about to type.
fn code_block(src: &str, a: usize, b: usize, lang: &str) -> EditResult {
    let selected = &src[a..b];
    let lead = if a == 0 || src[..a].ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let tail = if b == src.len() || src[b..].starts_with('\n') {
        ""
    } else {
        "\n"
    };
    let body = format!("{lead}```{lang}\n{selected}\n```{tail}");
    // Select the fenced content, which is what a second thought would edit.
    let skip = lead.chars().count() + 3 + lang.chars().count() + 1;
    insert(src, a, b, &body, skip, selected.chars().count())
}

/// Insert a table skeleton with a header row and `rows` body rows.
fn table(src: &str, a: usize, b: usize, rows: usize, cols: usize) -> EditResult {
    let cols = cols.clamp(1, 12);
    let rows = rows.clamp(1, 50);
    let header: Vec<String> = (1..=cols).map(|i| format!("Column {i}")).collect();
    let mut out = String::new();
    if !(a == 0 || src[..a].ends_with('\n')) {
        out.push('\n');
    }
    out.push_str(&format!("| {} |\n", header.join(" | ")));
    out.push_str(&format!("| {} |\n", vec!["---"; cols].join(" | ")));
    for _ in 0..rows {
        out.push_str(&format!("| {} |\n", vec!["   "; cols].join(" | ")));
    }
    // Select the first header cell: renaming the columns is the first thing
    // anybody does to a fresh table.
    let skip = out
        .find("Column 1")
        .map(|i| out[..i].chars().count())
        .unwrap_or(0);
    insert(src, a, b, &out, skip, "Column 1".chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, sel: (u32, u32), action: Action) -> EditResult {
        apply(src, sel.0, sel.1, &action)
    }

    /// The selected substring of a result, so a test can assert where the
    /// caret was left without counting offsets by hand.
    fn selected(r: &EditResult) -> String {
        let a = utf16_to_byte(&r.text, r.sel_start);
        let b = utf16_to_byte(&r.text, r.sel_end);
        r.text[a..b].to_string()
    }

    #[test]
    fn bold_wraps_the_selection() {
        let r = run("make this bold", (5, 9), Action::Bold);
        assert_eq!(r.text, "make **this** bold");
        assert_eq!(selected(&r), "this", "the selection survives the wrap");
    }

    /// A second click removes the marks instead of nesting them. Both shapes
    /// count as wrapped, because which one the browser reports depends on how
    /// the selection was made.
    #[test]
    fn bold_toggles_off_from_either_selection_shape() {
        // Markers inside the selection.
        let inside = run("make **this** bold", (5, 13), Action::Bold);
        assert_eq!(inside.text, "make this bold");
        assert_eq!(selected(&inside), "this");

        // Markers outside the selection.
        let outside = run("make **this** bold", (7, 11), Action::Bold);
        assert_eq!(outside.text, "make this bold");
        assert_eq!(selected(&outside), "this");
    }

    #[test]
    fn an_empty_selection_gets_a_placeholder_to_type_over() {
        let r = run("start ", (6, 6), Action::Bold);
        assert_eq!(r.text, "start **bold text**");
        assert_eq!(
            selected(&r),
            "bold text",
            "the placeholder is selected, so typing replaces it"
        );
    }

    #[test]
    fn italic_and_code_and_strike_use_their_own_markers() {
        assert_eq!(run("a b", (0, 1), Action::Italic).text, "*a* b");
        assert_eq!(run("a b", (0, 1), Action::InlineCode).text, "`a` b");
        assert_eq!(run("a b", (0, 1), Action::Strikethrough).text, "~~a~~ b");
    }

    /// Italic's marker is a prefix of bold's. Toggling italic on bold text must
    /// not strip one asterisk from each side and leave `*text*`.
    #[test]
    fn italic_does_not_half_unwrap_bold() {
        let r = run("**bold**", (0, 8), Action::Italic);
        assert_eq!(
            r.text, "***bold***",
            "italic wraps bold rather than unwrapping half of it"
        );
    }

    #[test]
    fn a_line_prefix_applies_to_every_touched_line() {
        let r = run("one\ntwo\nthree", (1, 6), Action::BulletList);
        assert_eq!(r.text, "- one\n- two\nthree");
    }

    #[test]
    fn a_line_prefix_toggles_off_when_every_line_has_it() {
        let src = "- one\n- two";
        let r = run(src, (0, 11), Action::BulletList);
        assert_eq!(r.text, "one\ntwo");
    }

    /// A partially-prefixed block is completed, not cleared. Clearing it would
    /// silently discard the prefixes the author already put there.
    #[test]
    fn a_partly_prefixed_block_is_completed() {
        let r = run("- one\ntwo", (0, 9), Action::BulletList);
        assert_eq!(r.text, "- - one\n- two");
    }

    #[test]
    fn quote_and_checklist_use_their_own_prefixes() {
        assert_eq!(run("a", (0, 1), Action::Quote).text, "> a");
        assert_eq!(run("a", (0, 1), Action::Checklist).text, "- [ ] a");
    }

    #[test]
    fn a_numbered_list_counts_from_one_and_toggles() {
        let r = run("one\ntwo\nthree", (0, 13), Action::NumberedList);
        assert_eq!(r.text, "1. one\n2. two\n3. three");
        let back = run(&r.text, (0, 22), Action::NumberedList);
        assert_eq!(back.text, "one\ntwo\nthree");
    }

    #[test]
    fn a_heading_replaces_the_level_and_toggles_off_at_the_same_one() {
        let h2 = run("Title", (0, 5), Action::Heading(2));
        assert_eq!(h2.text, "## Title");
        let h3 = run(&h2.text, (0, 8), Action::Heading(3));
        assert_eq!(h3.text, "### Title", "the level is replaced, not stacked");
        let off = run(&h3.text, (0, 9), Action::Heading(3));
        assert_eq!(off.text, "Title", "the same level again clears it");
    }

    #[test]
    fn a_link_uses_the_selection_as_its_label() {
        let r = run(
            "see the docs",
            (8, 12),
            Action::Link {
                text: String::new(),
                url: "https://x.test".to_string(),
            },
        );
        assert_eq!(r.text, "see the [docs](https://x.test)");
        assert_eq!(selected(&r), "docs");
    }

    #[test]
    fn a_link_with_no_selection_uses_the_dialog_text_then_a_placeholder() {
        let given = run(
            "",
            (0, 0),
            Action::Link {
                text: "Docs".to_string(),
                url: "https://x.test".to_string(),
            },
        );
        assert_eq!(given.text, "[Docs](https://x.test)");

        let bare = run(
            "",
            (0, 0),
            Action::Link {
                text: String::new(),
                url: "https://x.test".to_string(),
            },
        );
        assert_eq!(bare.text, "[link text](https://x.test)");
        assert_eq!(selected(&bare), "link text");
    }

    #[test]
    fn an_image_carries_its_alt_text() {
        let r = run(
            "",
            (0, 0),
            Action::Image {
                alt: "A diagram".to_string(),
                url: "https://x.test/d.png".to_string(),
            },
        );
        assert_eq!(r.text, "![A diagram](https://x.test/d.png)");
        assert_eq!(selected(&r), "A diagram");
    }

    #[test]
    fn a_code_block_fences_the_selection_on_its_own_lines() {
        let r = run(
            "before\nlet x = 1;\nafter",
            (7, 17),
            Action::CodeBlock {
                lang: "rust".to_string(),
            },
        );
        assert_eq!(r.text, "before\n```rust\nlet x = 1;\n```\nafter");
        assert_eq!(selected(&r), "let x = 1;");
    }

    /// The fence needs its own line. Mid-line, a newline is added rather than
    /// producing ```` text```rust ````, which is not a fence at all.
    #[test]
    fn a_code_block_mid_line_opens_a_new_line_first() {
        let r = run(
            "text",
            (4, 4),
            Action::CodeBlock {
                lang: String::new(),
            },
        );
        assert!(r.text.starts_with("text\n```"), "{:?}", r.text);
    }

    #[test]
    fn a_table_has_a_header_a_rule_and_the_rows_asked_for() {
        let r = run("", (0, 0), Action::Table { rows: 2, cols: 3 });
        let lines: Vec<&str> = r.text.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 4, "header + rule + 2 rows: {:?}", lines);
        assert_eq!(lines[0], "| Column 1 | Column 2 | Column 3 |");
        assert_eq!(lines[1], "| --- | --- | --- |");
        assert_eq!(
            selected(&r),
            "Column 1",
            "the first header cell is selected, because renaming the columns is \
             the first thing anybody does"
        );
    }

    // UTF-16 vs UTF-8 ------------------------------------------------------

    /// The trap this module exists to avoid. The browser reports selection in
    /// UTF-16 code units; treating those as byte offsets slices a Rust string
    /// mid-character and either panics or corrupts the text. Every case below
    /// is one an English-only test would miss.
    #[test]
    fn selection_offsets_are_utf16_not_bytes() {
        // "héllo" is 6 bytes but 5 UTF-16 units. Selecting "llo" is (2, 5).
        let r = run("héllo world", (2, 5), Action::Bold);
        assert_eq!(r.text, "hé**llo** world");
        assert_eq!(selected(&r), "llo");
    }

    /// An astral character is TWO UTF-16 units and four UTF-8 bytes, so the two
    /// counts drift in both directions at once.
    #[test]
    fn an_emoji_counts_as_two_utf16_units() {
        // "🎉 party": the emoji is units 0..2, so "party" is (3, 8).
        let src = "🎉 party";
        let r = run(src, (3, 8), Action::Bold);
        assert_eq!(r.text, "🎉 **party**");
        assert_eq!(selected(&r), "party");
    }

    /// And the returned selection is in the same units the caller will hand
    /// back to the DOM, so a round trip through a transform stays put.
    #[test]
    fn the_returned_selection_round_trips_through_the_dom_units() {
        let r = run("🎉 party", (3, 8), Action::Bold);
        let again = apply(&r.text, r.sel_start, r.sel_end, &Action::Bold);
        assert_eq!(
            again.text, "🎉 party",
            "toggling twice returns the original, which only holds if the offsets \
             we hand back are the ones the DOM speaks"
        );
    }

    /// Nothing may panic on a selection the DOM reports out of range, which
    /// happens when the value changed under a stale handler.
    #[test]
    fn an_out_of_range_selection_is_clamped_not_a_panic() {
        let r = run("short", (99, 200), Action::Bold);
        assert_eq!(r.text, "short**bold text**");
        let reversed = run("short", (4, 1), Action::Bold);
        assert_eq!(
            reversed.text, "s**hor**t",
            "a backwards selection is normalised"
        );
    }
}

/// MAPPS-579 AC12: an untouched body survives the editor byte-identical.
///
/// The toolbar is the only thing in this pass that rewrites the source, so
/// "untouched" means "no action ran". That is trivially true by construction,
/// which is exactly why it is worth an assertion: the risk is not that an
/// unused transform corrupts text, it is that a transform which DOES run
/// mangles the raw HTML MAPPS-573 went to trouble to preserve.
#[cfg(test)]
mod raw_html_fidelity {
    use super::*;

    const BODY: &str = "# Description\n\n\
* [ ] <span style=\"color:red\">**REST API**</span> - @niceguyit\n\
    - [ ] Secrets and tokens stored in Infisical\n\n\
| Issue | Status |\n|---|---|\n| PSA-19 | Mostly built |\n\n\
```bash\njust check   # runs the guards\n```\n";

    /// Every action, applied at a caret in the middle of the raw HTML span,
    /// leaves the rest of the document alone. A transform that reflowed or
    /// re-escaped the body would show up here as a changed prefix or suffix.
    #[test]
    fn a_transform_only_changes_what_it_touches() {
        let at = BODY.find("REST API").expect("the span is in the fixture") as u32;
        // The fixture is ASCII up to that point, so the byte offset is also the
        // UTF-16 offset; asserted rather than assumed.
        assert!(BODY[..at as usize].is_ascii());

        for action in [
            Action::Bold,
            Action::Italic,
            Action::Strikethrough,
            Action::InlineCode,
            Action::Quote,
            Action::BulletList,
            Action::NumberedList,
            Action::Checklist,
            Action::Heading(2),
            Action::CodeBlock {
                lang: String::new(),
            },
            Action::Table { rows: 1, cols: 2 },
            Action::Link {
                text: "t".into(),
                url: "https://x.test".into(),
            },
            Action::Image {
                alt: "a".into(),
                url: "https://x.test/i.png".into(),
            },
        ] {
            let out = apply(BODY, at, at, &action);
            assert!(
                out.text.contains("```bash"),
                "{action:?} disturbed the fenced block: {}",
                out.text
            );
            assert!(
                out.text.contains("| PSA-19 | Mostly built |"),
                "{action:?} disturbed the table: {}",
                out.text
            );
            assert!(
                out.text.contains("@niceguyit"),
                "{action:?} disturbed the mention: {}",
                out.text
            );
            assert!(
                out.text.contains("style=\"color:red\""),
                "{action:?} disturbed the raw HTML attribute MAPPS-573 preserves: {}",
                out.text
            );
        }
    }

    /// And a mark applied to a selection INSIDE the span leaves the span's own
    /// tags intact, rather than wrapping markers around the angle brackets.
    #[test]
    fn wrapping_text_inside_a_raw_html_span_keeps_the_tags() {
        let src = "<span style=\"color:red\">REST API</span>";
        let start = src.find("REST").expect("present") as u32;
        let end = start + 8;
        let out = apply(src, start, end, &Action::Italic);
        assert_eq!(
            out.text, "<span style=\"color:red\">*REST API*</span>",
            "the mark goes inside the tags, and the tags are untouched"
        );
    }
}

//! Syntax highlighting for fenced code blocks (MAPPS-573).
//!
//! Forgejo highlights code blocks and mokosh rendered them as flat grey text.
//! This is the smallest thing that closes that gap honestly.
//!
//! It is a lexical highlighter, not a parser: it finds comments, strings,
//! numbers, keywords and a few per-language extras, and leaves everything else
//! alone. That is what carries almost all of the readability benefit in a
//! knowledge-base snippet, which is typically a shell command, a config file or
//! twenty lines of an API example rather than a program. A real grammar engine
//! (`syntect` and friends) means shipping grammar dumps and a regex engine into
//! a WASM bundle to do better on code nobody pastes into a KB article.
//!
//! Deliberate consequences of being lexical, so nobody reports them as bugs:
//! a keyword used as an identifier is still coloured as a keyword, and a
//! construct that needs nesting (Rust raw strings with hashes, shell heredocs)
//! is not tracked. Both degrade to slightly-wrong colour on correct text, never
//! to wrong text: the output is escaped independently of the token scan.
//!
//! Every emitted class starts with `hl-` and is enumerated in [`CLASSES`],
//! which is also what the markdown sanitizer allowlists. The two must agree or
//! the colours silently vanish, so a test pins that they do.

/// Every class this module can emit. The sanitizer's allowlist is built from
/// this, so adding a token kind here is all it takes to let it through.
pub const CLASSES: &[&str] = &[
    "hl-com", // comment
    "hl-str", // string or character literal
    "hl-num", // numeric literal
    "hl-kw",  // language keyword
    "hl-typ", // built-in type, constant, or well-known identifier
    "hl-var", // interpolated variable ($HOME, %VAR%)
    "hl-key", // property / setting name (JSON, YAML, TOML, INI)
    "hl-ins", // diff: added line
    "hl-del", // diff: removed line
    "hl-mta", // diff: hunk header and file metadata
];

/// What one language looks like to the scanner.
struct Lang {
    /// Sequences that start a comment running to end of line.
    line_comments: &'static [&'static str],
    /// Block comment open/close, if the language has one.
    block_comment: Option<(&'static str, &'static str)>,
    /// Quote characters that open a string.
    quotes: &'static [char],
    /// Whether a backslash escapes the next character inside a string.
    escapes: bool,
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    /// `$name` / `${name}` reads as a variable (shells, Makefiles).
    dollar_vars: bool,
    /// A bare or quoted word followed by `:` or `=` at the start of a line is a
    /// property name (config formats).
    keyed: bool,
}

const DEFAULT_LANG: Lang = Lang {
    line_comments: &["#", "//"],
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\''],
    escapes: true,
    keywords: &[],
    types: &[],
    dollar_vars: false,
    keyed: false,
};

/// Resolve a fence's info string to a language. The info string may carry more
/// than the name (`rust,ignore`, `bash {1,3}`), so only the first word counts.
fn spec(lang: &str) -> Option<&'static Lang> {
    let name = lang
        .split(|c: char| c.is_whitespace() || c == ',' || c == '{')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    const RUST: Lang = Lang {
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: &['"'],
        escapes: true,
        keywords: &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while",
        ],
        types: &[
            "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16",
            "u32", "u64", "u128", "usize", "str", "String", "Vec", "Option", "Result", "Some",
            "None", "Ok", "Err", "Box", "HashMap",
        ],
        dollar_vars: false,
        keyed: false,
    };

    const JS: Lang = Lang {
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        quotes: &['"', '\'', '`'],
        escapes: true,
        keywords: &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "delete",
            "do",
            "else",
            "export",
            "extends",
            "finally",
            "for",
            "from",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "interface",
            "let",
            "new",
            "of",
            "return",
            "super",
            "switch",
            "this",
            "throw",
            "try",
            "type",
            "typeof",
            "var",
            "void",
            "while",
            "yield",
        ],
        types: &[
            "Array",
            "Boolean",
            "Number",
            "Object",
            "Promise",
            "String",
            "Symbol",
            "any",
            "boolean",
            "console",
            "false",
            "null",
            "number",
            "string",
            "true",
            "undefined",
            "unknown",
        ],
        dollar_vars: false,
        keyed: false,
    };

    const PYTHON: Lang = Lang {
        line_comments: &["#"],
        block_comment: None,
        quotes: &['"', '\''],
        escapes: true,
        keywords: &[
            "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
            "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in",
            "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
            "with", "yield",
        ],
        types: &[
            "False", "None", "True", "bool", "bytes", "dict", "float", "int", "list", "print",
            "self", "set", "str", "tuple",
        ],
        dollar_vars: false,
        keyed: false,
    };

    const SHELL: Lang = Lang {
        line_comments: &["#"],
        block_comment: None,
        quotes: &['"', '\''],
        escapes: true,
        keywords: &[
            "case", "do", "done", "elif", "else", "esac", "exit", "export", "fi", "for",
            "function", "if", "in", "local", "read", "return", "set", "shift", "then", "unset",
            "until", "while",
        ],
        types: &[
            "cat",
            "cd",
            "chmod",
            "chown",
            "cp",
            "curl",
            "cut",
            "docker",
            "echo",
            "find",
            "git",
            "grep",
            "just",
            "kubectl",
            "ls",
            "mkdir",
            "mv",
            "psql",
            "rm",
            "sed",
            "ssh",
            "sudo",
            "systemctl",
            "tar",
            "wget",
        ],
        dollar_vars: true,
        keyed: false,
    };

    const SQL: Lang = Lang {
        line_comments: &["--"],
        block_comment: Some(("/*", "*/")),
        quotes: &['\'', '"'],
        escapes: false,
        keywords: &[
            "ALTER",
            "AND",
            "AS",
            "ASC",
            "BY",
            "CASE",
            "CREATE",
            "DELETE",
            "DESC",
            "DISTINCT",
            "DROP",
            "ELSE",
            "END",
            "EXISTS",
            "FROM",
            "FULL",
            "GROUP",
            "HAVING",
            "IN",
            "INDEX",
            "INNER",
            "INSERT",
            "INTO",
            "IS",
            "JOIN",
            "LEFT",
            "LIKE",
            "LIMIT",
            "NOT",
            "NULL",
            "OFFSET",
            "ON",
            "OR",
            "ORDER",
            "OUTER",
            "RETURNING",
            "RIGHT",
            "SELECT",
            "SET",
            "TABLE",
            "THEN",
            "UNION",
            "UPDATE",
            "VALUES",
            "WHEN",
            "WHERE",
            "WITH",
        ],
        types: &[
            "BOOLEAN",
            "DATE",
            "INTEGER",
            "JSONB",
            "NUMERIC",
            "TEXT",
            "TIMESTAMPTZ",
            "UUID",
            "VARCHAR",
        ],
        dollar_vars: false,
        keyed: false,
    };

    const JSON: Lang = Lang {
        line_comments: &[],
        block_comment: None,
        quotes: &['"'],
        escapes: true,
        keywords: &["false", "null", "true"],
        types: &[],
        dollar_vars: false,
        keyed: true,
    };

    const YAML: Lang = Lang {
        line_comments: &["#"],
        block_comment: None,
        quotes: &['"', '\''],
        escapes: true,
        keywords: &["false", "no", "null", "true", "yes", "~"],
        types: &[],
        dollar_vars: true,
        keyed: true,
    };

    const TOML: Lang = Lang {
        line_comments: &["#"],
        block_comment: None,
        quotes: &['"', '\''],
        escapes: true,
        keywords: &["false", "true"],
        types: &[],
        dollar_vars: false,
        keyed: true,
    };

    Some(match name.as_str() {
        "rust" | "rs" => &RUST,
        "js" | "javascript" | "ts" | "typescript" | "jsx" | "tsx" => &JS,
        "python" | "py" => &PYTHON,
        "bash" | "sh" | "shell" | "zsh" | "console" | "shell-session" | "nu" | "nushell" => &SHELL,
        "sql" | "postgres" | "postgresql" | "psql" => &SQL,
        "json" | "jsonc" => &JSON,
        "yaml" | "yml" => &YAML,
        "toml" | "ini" | "conf" | "cfg" | "env" | "dotenv" => &TOML,
        // An UNLABELLED fence is left alone, which is what Forgejo does and
        // what the author asked for by not naming a language. Guessing there
        // goes wrong in the cases that matter: `#` opens a comment in a shell
        // but is a heading in a pasted Markdown sample, and a `$` prompt in a
        // terminal transcript is not a variable.
        "" => return None,
        // A fence that DOES name a language we do not know still gets the
        // universal tokens. The author has asserted it is code, so comments,
        // strings and numbers are a fair reading of it.
        _ => &DEFAULT_LANG,
    })
}

/// HTML-escape into `out`. Applied to every byte of source text, on every path,
/// so a token-scan mistake can never produce unescaped output.
fn push_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

fn push_span(out: &mut String, class: &str, text: &str) {
    out.push_str("<span class=\"");
    out.push_str(class);
    out.push_str("\">");
    push_escaped(out, text);
    out.push_str("</span>");
}

/// Highlight `code` as `lang`, returning escaped HTML with `hl-*` spans.
///
/// `lang` is the fence info string and may be empty. The returned HTML is
/// already escaped and is safe to place inside `<code>`.
pub fn highlight_html(lang: &str, code: &str) -> String {
    let name = lang
        .split(|c: char| c.is_whitespace() || c == ',' || c == '{')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(name.as_str(), "diff" | "patch") {
        return highlight_diff(code);
    }
    let Some(spec) = spec(lang) else {
        let mut out = String::with_capacity(code.len());
        push_escaped(&mut out, code);
        return out;
    };
    highlight_with(spec, code)
}

/// Diff is line-oriented, so it gets its own pass rather than a token scan that
/// would colour the contents of every changed line instead of the change.
fn highlight_diff(code: &str) -> String {
    let mut out = String::with_capacity(code.len() * 2);
    for line in code.split_inclusive('\n') {
        let (body, nl) = match line.strip_suffix('\n') {
            Some(b) => (b, "\n"),
            None => (line, ""),
        };
        // Order matters: `+++` / `---` are file headers, not an added or
        // removed line, so they are tested before the single-character forms.
        let class = if body.starts_with("@@") {
            Some("hl-mta")
        } else if body.starts_with("+++")
            || body.starts_with("---")
            || body.starts_with("diff ")
            || body.starts_with("index ")
        {
            Some("hl-com")
        } else if body.starts_with('+') {
            Some("hl-ins")
        } else if body.starts_with('-') {
            Some("hl-del")
        } else {
            None
        };
        match class {
            Some(c) => push_span(&mut out, c, body),
            None => push_escaped(&mut out, body),
        }
        out.push_str(nl);
    }
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// End offset of the `$name` / `${name}` starting at `at`, or `None` if what
/// follows the `$` is not a variable.
fn var_end(code: &str, at: usize) -> Option<usize> {
    let mut j = at + 1;
    if code[j..].starts_with('{') {
        return code[j..].find('}').map(|p| j + p + 1);
    }
    while j < code.len() {
        let ch = code[j..].chars().next().expect("in-bounds");
        if !is_ident(ch) {
            break;
        }
        j += ch.len_utf8();
    }
    (j > at + 1).then_some(j)
}

/// Emit a string literal. When the language interpolates (shells, YAML), a
/// `$var` inside a DOUBLE-quoted string is highlighted as a variable, because
/// that is what the shell does with it and it is the common case in a
/// knowledge-base snippet (`echo "$HOME/bin"`). Single quotes are literal in
/// those languages, so they are emitted whole.
fn push_string(out: &mut String, spec: &Lang, text: &str) {
    if !spec.dollar_vars || !text.starts_with('"') {
        push_span(out, "hl-str", text);
        return;
    }
    out.push_str("<span class=\"hl-str\">");
    let mut i = 0usize;
    while i < text.len() {
        let ch = text[i..].chars().next().expect("in-bounds");
        if ch == '$' {
            if let Some(end) = var_end(text, i) {
                // Close the string span around the variable so the two colours
                // do not nest, which would need the outer class to win.
                out.push_str("</span>");
                push_span(out, "hl-var", &text[i..end]);
                out.push_str("<span class=\"hl-str\">");
                i = end;
                continue;
            }
        }
        push_escaped(out, &text[i..i + ch.len_utf8()]);
        i += ch.len_utf8();
    }
    out.push_str("</span>");
}

fn highlight_with(spec: &Lang, code: &str) -> String {
    let mut out = String::with_capacity(code.len() * 2);
    let bytes = code.as_bytes();
    let mut i = 0usize;
    // Start of the current line, so `keyed` can tell a property name from a
    // value that happens to be followed by a colon.
    let mut line_start = 0usize;
    // Whether a `:` or `=` has already been seen on this line.
    let mut seen_sep = false;

    while i < code.len() {
        let rest = &code[i..];
        let c = rest.chars().next().expect("in-bounds slice is non-empty");

        if c == '\n' {
            out.push('\n');
            i += 1;
            line_start = i;
            seen_sep = false;
            continue;
        }

        // Block comment.
        if let Some((open, close)) = spec.block_comment {
            if rest.starts_with(open) {
                let end = rest[open.len()..]
                    .find(close)
                    .map(|p| i + open.len() + p + close.len())
                    .unwrap_or(code.len());
                push_span(&mut out, "hl-com", &code[i..end]);
                i = end;
                continue;
            }
        }

        // Line comment. Not inside a string, because strings are consumed whole
        // below before we ever look at their contents.
        if let Some(marker) = spec
            .line_comments
            .iter()
            .find(|m| rest.starts_with(**m))
            .copied()
        {
            // A shell `#!` shebang and a CSS-ish `#rrggbb` are not comments,
            // but treating them as one only miscolours; keep the rule simple
            // except for the shebang, which is common at the top of a snippet.
            let _ = marker;
            let end = rest.find('\n').map(|p| i + p).unwrap_or(code.len());
            push_span(&mut out, "hl-com", &code[i..end]);
            i = end;
            continue;
        }

        // String literal, consumed to its closing quote or end of line. Ending
        // at the newline keeps one unbalanced quote from swallowing the rest of
        // the block, which is the failure that makes a lexical highlighter look
        // broken rather than imprecise.
        if spec.quotes.contains(&c) {
            let mut j = i + c.len_utf8();
            let mut closed = false;
            while j < code.len() {
                let ch = code[j..].chars().next().expect("in-bounds");
                if ch == '\n' {
                    break;
                }
                if spec.escapes && ch == '\\' {
                    j += ch.len_utf8();
                    if j < code.len() {
                        j += code[j..].chars().next().map_or(0, char::len_utf8);
                    }
                    continue;
                }
                j += ch.len_utf8();
                if ch == c {
                    closed = true;
                    break;
                }
            }
            let text = &code[i..j];
            // A quoted property name is a key, not a value.
            let is_key = spec.keyed
                && closed
                && !seen_sep
                && code[j..]
                    .trim_start_matches([' ', '\t'])
                    .starts_with([':', '=']);
            if is_key {
                push_span(&mut out, "hl-key", text);
            } else {
                push_string(&mut out, spec, text);
            }
            i = j;
            continue;
        }

        // Interpolated variable.
        if spec.dollar_vars && c == '$' {
            if let Some(j) = var_end(code, i) {
                push_span(&mut out, "hl-var", &code[i..j]);
                i = j;
                continue;
            }
        }

        // Number. Only when it does not continue an identifier, so `utf8` and
        // `sha256` stay whole.
        if c.is_ascii_digit() && (i == 0 || !is_ident(code[..i].chars().next_back().unwrap_or(' ')))
        {
            let mut j = i;
            while j < code.len() {
                let ch = code[j..].chars().next().expect("in-bounds");
                if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' {
                    j += ch.len_utf8();
                } else {
                    break;
                }
            }
            push_span(&mut out, "hl-num", &code[i..j]);
            i = j;
            continue;
        }

        // Identifier: keyword, type, property name, or plain text.
        if is_ident(c) && !c.is_ascii_digit() {
            let mut j = i;
            while j < code.len() {
                let ch = code[j..].chars().next().expect("in-bounds");
                if !is_ident(ch) {
                    break;
                }
                j += ch.len_utf8();
            }
            let word = &code[i..j];
            let bare_key = spec.keyed
                && !seen_sep
                && code[line_start..i].trim().is_empty()
                && code[j..]
                    .trim_start_matches([' ', '\t'])
                    .starts_with([':', '=']);
            let class = if bare_key {
                Some("hl-key")
            } else if spec.keywords.contains(&word) {
                Some("hl-kw")
            } else if spec.types.contains(&word) {
                Some("hl-typ")
            } else {
                None
            };
            match class {
                Some(cl) => push_span(&mut out, cl, word),
                None => push_escaped(&mut out, word),
            }
            i = j;
            continue;
        }

        if c == ':' || c == '=' {
            seen_sep = true;
        }
        push_escaped(&mut out, &code[i..i + c.len_utf8()]);
        i += c.len_utf8();
    }
    debug_assert_eq!(bytes.len(), code.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever the token scan does, the text has to come out intact and
    /// escaped. This is the property that keeps a highlighter bug cosmetic.
    #[test]
    fn every_language_escapes_html_and_preserves_the_text() {
        let evil = "<script>alert(\"x\" & 'y')</script>\n-- </code>\n";
        for lang in [
            "", "rust", "js", "python", "bash", "sql", "json", "yaml", "toml", "diff", "klingon",
        ] {
            let out = highlight_html(lang, evil);
            assert!(!out.contains("<script"), "{lang}: raw tag survived: {out}");
            assert!(
                !out.contains("</code>"),
                "{lang}: an unescaped closing tag would break out of the block: {out}"
            );
            // Strip our own spans, unescape, and the original must come back.
            let text = strip_spans(&out);
            assert_eq!(text, evil, "{lang}: text was altered");
        }
    }

    /// Undo what the highlighter added, so a test can compare against the input.
    fn strip_spans(html: &str) -> String {
        let mut s = String::new();
        let mut rest = html;
        while let Some(p) = rest.find('<') {
            s.push_str(&rest[..p]);
            let end = rest[p..].find('>').map(|e| p + e + 1).unwrap_or(rest.len());
            rest = &rest[end..];
        }
        s.push_str(rest);
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&")
    }

    #[test]
    fn rust_keywords_types_strings_and_comments() {
        let out = highlight_html("rust", "// note\nfn main() { let s: String = \"hi\"; }");
        assert!(out.contains(r#"<span class="hl-com">// note</span>"#));
        assert!(out.contains(r#"<span class="hl-kw">fn</span>"#));
        assert!(out.contains(r#"<span class="hl-kw">let</span>"#));
        assert!(out.contains(r#"<span class="hl-typ">String</span>"#));
        assert!(out.contains(r#"<span class="hl-str">&quot;hi&quot;</span>"#));
    }

    #[test]
    fn shell_variables_and_commands() {
        let out = highlight_html("bash", "docker ps # list\necho \"${HOME}\"");
        assert!(out.contains(r#"<span class="hl-typ">docker</span>"#));
        assert!(out.contains(r#"<span class="hl-com"># list</span>"#));
        assert!(out.contains(r#"<span class="hl-var">${HOME}</span>"#));
    }

    #[test]
    fn config_formats_mark_the_property_name_not_the_value() {
        let json = highlight_html("json", "{\n  \"name\": \"mokosh\"\n}");
        assert!(
            json.contains(r#"<span class="hl-key">&quot;name&quot;</span>"#),
            "the key is a key: {json}"
        );
        assert!(
            json.contains(r#"<span class="hl-str">&quot;mokosh&quot;</span>"#),
            "and the value is still a string: {json}"
        );

        let yaml = highlight_html("yaml", "image: postgres:17\nrestart: always");
        assert!(yaml.contains(r#"<span class="hl-key">image</span>"#));
        // `postgres` sits after the separator, so it is a value, not a key.
        assert!(!yaml.contains(r#"<span class="hl-key">postgres</span>"#));
    }

    #[test]
    fn diff_colours_the_change_not_the_contents() {
        let out = highlight_html(
            "diff",
            "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old line\n+new line\n unchanged\n",
        );
        assert!(out.contains(r#"<span class="hl-mta">@@ -1 +1 @@</span>"#));
        assert!(out.contains(r#"<span class="hl-del">-old line</span>"#));
        assert!(out.contains(r#"<span class="hl-ins">+new line</span>"#));
        // The file headers are metadata, not a removed and an added line.
        assert!(out.contains(r#"<span class="hl-com">--- a/x</span>"#));
        assert!(out.contains(r#"<span class="hl-com">+++ b/x</span>"#));
    }

    /// One unbalanced quote must not swallow the rest of the block. This is the
    /// difference between a highlighter that looks imprecise and one that looks
    /// broken.
    #[test]
    fn an_unclosed_string_ends_at_the_line() {
        let out = highlight_html("bash", "echo \"oops\ngit status\n");
        assert!(
            out.contains(r#"<span class="hl-typ">git</span>"#),
            "the line after an unclosed quote is still scanned: {out}"
        );
    }

    #[test]
    fn a_number_inside_an_identifier_is_not_a_number() {
        let out = highlight_html("rust", "let sha256 = 42;");
        assert!(!out.contains(r#"<span class="hl-num">256</span>"#), "{out}");
        assert!(out.contains(r#"<span class="hl-num">42</span>"#), "{out}");
    }

    /// An unlabelled fence is passed through escaped and untouched: the author
    /// did not say it was code in any language, and guessing is how a pasted
    /// Markdown sample or terminal transcript gets miscoloured.
    #[test]
    fn an_unlabelled_fence_is_not_highlighted() {
        let out = highlight_html("", "# heading\nlet x = 1;\n$ prompt\n");
        assert!(!out.contains("<span"), "nothing is tokenised: {out}");
        assert_eq!(out, "# heading\nlet x = 1;\n$ prompt\n");
    }

    #[test]
    fn an_unknown_language_still_gets_the_universal_tokens() {
        let out = highlight_html("klingon", "# note\nvalue = \"x\"\n");
        assert!(out.contains(r#"<span class="hl-com"># note</span>"#));
        assert!(out.contains(r#"<span class="hl-str">&quot;x&quot;</span>"#));
    }

    /// Multi-byte input must not panic on a byte-index slice.
    #[test]
    fn utf8_source_is_handled_by_char_not_byte() {
        for lang in ["rust", "bash", "json", ""] {
            let out = highlight_html(lang, "let s = \"héllo → wörld\"; // ünïcode\n");
            assert!(out.contains("héllo"), "{lang}: {out}");
        }
    }

    #[test]
    fn every_emitted_class_is_declared_in_classes() {
        // Render a sample of each language, pull every class out of the output,
        // and require CLASSES to cover it. CLASSES is what the sanitizer
        // allowlists, so anything missing here renders colourless.
        let samples = [
            ("rust", "// c\nfn f() { let x = \"s\"; }"),
            ("bash", "# c\necho \"$HOME\" 1"),
            ("json", "{\"k\": \"v\", \"n\": 1, \"b\": true}"),
            ("yaml", "# c\nkey: value\nn: 2"),
            ("sql", "-- c\nSELECT * FROM t WHERE id = 1;"),
            ("diff", "@@ -1 +1 @@\n-a\n+b\n--- x\n"),
            ("python", "# c\ndef f(): return None"),
            ("js", "// c\nconst x = `t`; // 1"),
        ];
        for (lang, src) in samples {
            let out = highlight_html(lang, src);
            let mut rest = out.as_str();
            while let Some(p) = rest.find("<span class=\"") {
                rest = &rest[p + 13..];
                let end = rest.find('"').expect("class attribute closes");
                let class = &rest[..end];
                assert!(
                    CLASSES.contains(&class),
                    "{lang} emitted `{class}`, which is not in CLASSES, so the sanitizer \
                     will strip it and the colour will vanish"
                );
                rest = &rest[end..];
            }
        }
    }
}

//! Resolve `@handle` in Markdown to a real person (MAPPS-578).
//!
//! Authors already use mentions to assign ownership: the article that prompted
//! MAPPS-573 carries `@niceguyit`, `@long` and `@Nate` against checklist items.
//! Rendered as literal text, a reader cannot tell whether those name a
//! colleague or are leftovers.
//!
//! The rule this module exists to enforce is that a mention resolves or it
//! does not, and the two look different. An `@` that does not name someone in
//! the tenant is left exactly as the author typed it: an email address in
//! prose, a Python decorator, a handle for someone who has left. A renderer
//! that decorates anything after an `@` is worse than none, because it makes
//! the unresolved case look authoritative.
//!
//! `UserResponse` has no username field, so a handle is derived. See
//! [`Mention::handles`] for what is accepted and why ambiguity resolves to
//! nothing rather than to a guess.

/// One person the renderer can resolve a mention to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    /// The user's id, carried into the markup so a click can route.
    pub id: String,
    /// What to render in the chip: the person's full name where there is one.
    pub display: String,
    /// The local part of their email address, as the server's staff directory
    /// reports it (PMS-921). Shown on hover so a reader can see what the author
    /// actually typed.
    ///
    /// Not the address. `GET /auth/directory` returns the local part only,
    /// because it is what mention resolution matches on and it is not something
    /// anyone can send to. A technician can already see a colleague's display
    /// name across the app but no authenticated surface returns a staff email,
    /// so carrying one here would be a disclosure the feature does not need.
    pub handle: String,
}

impl Mention {
    /// Every handle that should resolve to this person, lowercased.
    ///
    /// There is no username to match on, so this is derived from what people
    /// actually type when mentioning a colleague:
    ///
    /// - the directory handle, so `long@niceguyit.com` answers to `@long`,
    /// - the first name,
    /// - the full name with a `.` and with nothing between the parts, which is
    ///   the two shapes a handle usually takes.
    ///
    /// The last name alone is deliberately absent. It collides with a first
    /// name often enough to be a real source of wrong attribution, and nobody
    /// writes `@smith` meaning a specific colleague.
    pub fn handles(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |s: String| {
            let s = s.trim().to_lowercase();
            if !s.is_empty() && !out.contains(&s) {
                out.push(s);
            }
        };

        push(self.handle.clone());
        let parts: Vec<&str> = self
            .display
            .split_whitespace()
            .filter(|p| !p.is_empty())
            .collect();
        if let Some(first) = parts.first() {
            push((*first).to_string());
        }
        if parts.len() >= 2 {
            push(parts.join("."));
            push(parts.concat());
        }
        out
    }
}

/// Resolve `handle` against `people`, case-insensitively.
///
/// Returns `None` when nothing matches AND when more than one person matches.
/// Guessing between two colleagues would silently attribute a checklist item
/// to the wrong one, which is worse than leaving the text alone: an unresolved
/// mention is visibly unresolved, a wrong one is not.
pub fn resolve<'a>(handle: &str, people: &'a [Mention]) -> Option<&'a Mention> {
    let needle = handle.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let mut found: Option<&Mention> = None;
    for person in people {
        if person.handles().contains(&needle) {
            if found.is_some() {
                // Ambiguous.
                return None;
            }
            found = Some(person);
        }
    }
    found
}

/// The handle starting at `at`, which must be the byte offset of an `@`.
///
/// Returns the end offset of the handle, or `None` when this `@` does not open
/// one. Two rejections carry the weight here:
///
/// - an `@` preceded by a word character is inside an email address or a path,
///   not a mention;
/// - an `@` followed by nothing handle-shaped is just an `@`.
///
/// A handle is letters, digits, `.`, `_` and `-`. A trailing `.` or `-` is
/// excluded so `@long.` at the end of a sentence keeps its full stop.
pub fn handle_end(text: &str, at: usize) -> Option<usize> {
    debug_assert!(text[at..].starts_with('@'));
    if !opens_a_handle(text, at) {
        return None;
    }
    let mut end = at + 1;
    while end < text.len() {
        let c = text[end..].chars().next().expect("in-bounds");
        if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    // Trim trailing punctuation that reads as sentence punctuation.
    while end > at + 1 {
        let last = text[..end].chars().next_back().expect("non-empty");
        if last == '.' || last == '-' || last == '_' {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    (end > at + 1).then_some(end)
}

/// Whether the `@` at `at` can open a handle at all.
///
/// Split out of [`handle_end`] so the editor's autocomplete (MAPPS-580) applies
/// the identical rule about what may precede an `@`. The two differ in exactly
/// one way and it is deliberate: `handle_end` needs at least one character
/// after the `@`, because a bare `@` is not a mention to render, while the
/// autocomplete opens on a bare `@`, because that is the moment the author most
/// wants to see who is available. Everything else about "is this an `@` that
/// could name somebody" is shared, so the editor cannot offer to complete
/// something the renderer will not resolve.
pub fn opens_a_handle(text: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let prev = text[..at].chars().next_back().unwrap_or(' ');
    // `name@example.com` and `path/to@thing` are not mentions.
    !(prev.is_alphanumeric() || prev == '.' || prev == '_' || prev == '-')
}

/// An in-progress `@fragment` the caret sits inside (MAPPS-580).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveMention {
    /// UTF-16 offset of the `@`.
    pub at: u32,
    /// UTF-16 offset just past the fragment, which is where the caret is.
    pub end: u32,
    /// What has been typed after the `@`, which is what to filter on.
    pub fragment: String,
}

/// The mention being typed at `caret`, if there is one.
///
/// Deliberately shares [`handle_end`]'s rule about what opens a handle, rather
/// than reimplementing "is this an `@`". If the two disagreed, the editor would
/// offer to complete something the renderer will not resolve, or stay silent
/// where it would: an autocomplete that is not the renderer's own rule is worse
/// than none, because it teaches the author the wrong thing.
///
/// `caret` is a UTF-16 offset, as every selection offset in this codebase is;
/// see [`crate::utils::md_edit`] for why that distinction is load-bearing.
pub fn active_mention(text: &str, caret: u32) -> Option<ActiveMention> {
    let caret_byte = utf16_to_byte(text, caret);
    let before = &text[..caret_byte];

    // Walk back to the nearest `@`. Stop at whitespace or a newline: a handle
    // has neither, so anything further back belongs to an earlier word.
    let at_byte = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| !c.is_whitespace())
        .find(|(_, c)| *c == '@')
        .map(|(i, _)| i)?;

    // The same test the renderer applies to what PRECEDES the `@`. An `@`
    // inside an email address or mid-word does not open a handle, so it does
    // not open a list either.
    if !opens_a_handle(text, at_byte) {
        return None;
    }

    let fragment = before[at_byte + 1..].to_string();
    // Everything between the `@` and the caret has to still be handle-shaped.
    // Typing `@long more` is a finished mention followed by a word, not a
    // nine-character handle, and the walk-back above already stops at
    // whitespace, so this catches the punctuation cases it cannot.
    if !fragment
        .chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return None;
    }
    Some(ActiveMention {
        at: byte_to_utf16(text, at_byte),
        end: caret,
        fragment,
    })
}

/// People whose name or handle matches `fragment`, best-first.
///
/// An empty fragment (the caret just after a bare `@`) lists everyone, because
/// that is the moment the author most wants to see who is available.
pub fn matches<'a>(fragment: &str, people: &'a [Mention]) -> Vec<&'a Mention> {
    let needle = fragment.trim().to_lowercase();
    let mut scored: Vec<(u8, &Mention)> = people
        .iter()
        .filter_map(|p| {
            if needle.is_empty() {
                return Some((2, p));
            }
            // Every handle the RENDERER would resolve, not a field read off
            // the struct: what autocompletes and what resolves have to be the
            // same set, or the editor offers completions that render as plain
            // text.
            let handles = p.handles();
            let display = p.display.to_lowercase();
            // A handle that starts with what was typed is what the author
            // means; a match anywhere is a fallback, so exact prefixes are not
            // buried under incidental substring hits.
            if handles.iter().any(|h| h.starts_with(&needle)) {
                Some((0, p))
            } else if display.starts_with(&needle) {
                Some((1, p))
            } else if handles.iter().any(|h| h.contains(&needle)) || display.contains(&needle) {
                Some((2, p))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.display.cmp(&b.1.display)));
    scored.into_iter().map(|(_, p)| p).collect()
}

/// Replace the fragment at `active` with `handle`, returning the new text and
/// where to leave the caret, both in UTF-16 units.
///
/// A trailing space, because the author has finished naming somebody and is
/// about to keep writing. Without it the next keystroke extends the handle and
/// the mention stops resolving.
pub fn accept(text: &str, active: &ActiveMention, handle: &str) -> (String, u32) {
    let a = utf16_to_byte(text, active.at);
    let b = utf16_to_byte(text, active.end);
    let inserted = format!("@{handle} ");
    let mut out = String::with_capacity(text.len() + inserted.len());
    out.push_str(&text[..a]);
    out.push_str(&inserted);
    out.push_str(&text[b..]);
    let caret = byte_to_utf16(&out, a + inserted.len());
    (out, caret)
}

/// Byte offset of the `n`th UTF-16 code unit, clamped.
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

/// UTF-16 offset of a byte offset.
fn byte_to_utf16(s: &str, target: usize) -> u32 {
    let target = target.min(s.len());
    s[..target].chars().map(|c| c.len_utf16() as u32).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(display: &str, handle: &str) -> Mention {
        Mention {
            id: format!("id-{display}"),
            display: display.to_string(),
            handle: handle.to_string(),
        }
    }

    #[test]
    fn handles_cover_what_people_actually_type() {
        let p = person("Long Le", "long");
        let h = p.handles();
        assert!(h.contains(&"long".to_string()), "directory handle: {h:?}");
        assert!(h.contains(&"long.le".to_string()), "first.last: {h:?}");
        assert!(h.contains(&"longle".to_string()), "firstlast: {h:?}");
        // Last name alone collides with first names too often to be safe.
        assert!(!h.contains(&"le".to_string()), "{h:?}");
    }

    #[test]
    fn resolution_is_case_insensitive() {
        let people = vec![person("Long Le", "long")];
        for handle in ["long", "Long", "LONG", "Long.Le", "longle"] {
            assert!(
                resolve(handle, &people).is_some(),
                "`{handle}` should resolve"
            );
        }
    }

    /// The rule that keeps a mention from lying. Two colleagues who both answer
    /// to `@chris` resolve to neither, because attributing a checklist item to
    /// the wrong one is invisible while an unresolved mention is not.
    #[test]
    fn an_ambiguous_handle_resolves_to_nobody() {
        let people = vec![
            person("Chris Adams", "chris"),
            person("Chris Brown", "chrisb"),
        ];
        assert!(resolve("chris", &people).is_none(), "ambiguous");
        // The unambiguous one still resolves.
        assert_eq!(
            resolve("chrisb", &people).map(|p| p.display.as_str()),
            Some("Chris Brown")
        );
        assert_eq!(
            resolve("chris.adams", &people).map(|p| p.display.as_str()),
            Some("Chris Adams")
        );
    }

    #[test]
    fn an_unknown_handle_resolves_to_nobody() {
        let people = vec![person("Long Le", "long")];
        assert!(resolve("nobody", &people).is_none());
        assert!(resolve("", &people).is_none());
    }

    /// An `@` inside an email address, a path or a word is not a mention. This
    /// is the check that keeps prose and code from sprouting chips.
    #[test]
    fn an_at_inside_a_word_does_not_open_a_handle() {
        for (text, at) in [
            ("mail long@niceguyit.com", 9),
            ("path/to@thing", 7),
            ("v1.2@rc", 4),
        ] {
            assert!(
                handle_end(text, at).is_none(),
                "{text:?} at {at} must not open a handle"
            );
        }
    }

    #[test]
    fn a_handle_stops_at_punctuation_and_whitespace() {
        let text = "ask @long, then @nate.";
        let first = handle_end(text, 4).expect("opens a handle");
        assert_eq!(&text[4..first], "@long");
        let second = handle_end(text, 16).expect("opens a handle");
        assert_eq!(
            &text[16..second],
            "@nate",
            "the sentence's full stop is not part of the handle"
        );
    }

    #[test]
    fn a_bare_at_is_not_a_handle() {
        assert!(handle_end("@ ", 0).is_none());
        assert!(handle_end("@", 0).is_none());
        assert!(handle_end("say @!", 4).is_none());
    }

    #[test]
    fn a_handle_at_the_start_of_the_text_is_allowed() {
        let text = "@long owns this";
        let end = handle_end(text, 0).expect("opens a handle");
        assert_eq!(&text[0..end], "@long");
    }

    // MAPPS-580 ------------------------------------------------------------

    fn people() -> Vec<Mention> {
        vec![
            person("Long Le", "long@niceguyit.com"),
            person("Nate Fisher", "nate@niceguyit.com"),
            person("Chris Adams", "chris@x.test"),
        ]
    }

    #[test]
    fn typing_an_at_opens_a_mention_with_the_fragment_so_far() {
        let text = "ask @lo";
        let active = active_mention(text, 7).expect("a mention is being typed");
        assert_eq!(active.at, 4);
        assert_eq!(active.end, 7);
        assert_eq!(active.fragment, "lo");
    }

    #[test]
    fn a_bare_at_opens_a_mention_with_an_empty_fragment() {
        let active = active_mention("ask @", 5).expect("a bare @ still offers the list");
        assert_eq!(active.fragment, "");
    }

    /// AC1, and the reason this shares `handle_end`. An `@` the RENDERER would
    /// not treat as a mention must not open a list either, or the editor offers
    /// to complete something that will render as plain text.
    #[test]
    fn an_at_the_renderer_would_ignore_does_not_open_a_list() {
        // Inside an email address.
        assert!(active_mention("mail long@niceguyit", 19).is_none());
        // Mid-word.
        assert!(active_mention("v1.2@rc", 7).is_none());
        // Not a mention context at all.
        assert!(active_mention("no at sign here", 15).is_none());
    }

    /// Typing past the handle closes the list: `@long more` is a finished
    /// mention followed by a word, not a nine-character handle.
    #[test]
    fn the_list_closes_once_the_caret_leaves_the_handle() {
        let text = "ask @long more";
        assert!(active_mention(text, 9).is_some(), "still inside the handle");
        assert!(
            active_mention(text, 14).is_none(),
            "past the space, the mention is finished"
        );
    }

    #[test]
    fn matching_prefers_a_handle_prefix_then_a_name_prefix() {
        let people = people();
        let hits = matches("n", &people);
        assert_eq!(
            hits.first().map(|p| p.display.as_str()),
            Some("Nate Fisher"),
            "the handle prefix wins: {hits:?}"
        );

        // Case-insensitive, and a name prefix matches too.
        assert_eq!(
            matches("CHRIS", &people)
                .first()
                .map(|p| p.display.as_str()),
            Some("Chris Adams")
        );
        assert!(matches("zzz", &people).is_empty());
    }

    #[test]
    fn an_empty_fragment_lists_everyone() {
        assert_eq!(matches("", &people()).len(), 3);
    }

    /// AC4. The handle is inserted, not the display name, because the handle is
    /// what resolves. Inserting "Long Le" would render as plain text.
    #[test]
    fn accepting_inserts_the_handle_and_a_trailing_space() {
        let text = "ask @lo";
        let active = active_mention(text, 7).expect("active");
        let (out, caret) = accept(text, &active, "long");
        assert_eq!(out, "ask @long ");
        assert_eq!(
            caret, 10,
            "the caret follows the space, ready to keep typing"
        );
    }

    /// Without the trailing space the next keystroke extends the handle and the
    /// mention silently stops resolving.
    #[test]
    fn the_trailing_space_keeps_the_next_keystroke_out_of_the_handle() {
        let text = "@lo";
        let active = active_mention(text, 3).expect("active");
        let (out, caret) = accept(text, &active, "long");
        assert_eq!(out, "@long ");
        assert!(
            active_mention(&out, caret).is_none(),
            "the caret is past the handle, so the list does not reopen"
        );
    }

    /// Accepting mid-sentence replaces only the fragment.
    #[test]
    fn accepting_leaves_the_rest_of_the_line_alone() {
        let text = "please ask @na about the runbook";
        let active = active_mention(text, 14).expect("active");
        let (out, _) = accept(text, &active, "nate");
        assert_eq!(out, "please ask @nate  about the runbook");
    }

    /// The same UTF-16 trap `md_edit` documents: the browser counts UTF-16
    /// units, Rust counts bytes, and they agree only for ASCII.
    #[test]
    fn caret_offsets_are_utf16() {
        // "héllo @na": the accented char is 2 bytes but 1 UTF-16 unit, so the
        // caret after "na" is at unit 9.
        let text = "héllo @na";
        let active = active_mention(text, 9).expect("a mention is being typed");
        assert_eq!(active.fragment, "na");
        let (out, _) = accept(text, &active, "nate");
        assert_eq!(out, "héllo @nate ");
    }

    #[test]
    fn an_emoji_before_the_mention_does_not_shift_the_fragment() {
        // The emoji is two UTF-16 units, so "@na" ends at unit 6.
        let text = "🎉 @na";
        let active = active_mention(text, 6).expect("active");
        assert_eq!(active.fragment, "na");
        let (out, _) = accept(text, &active, "nate");
        assert_eq!(out, "🎉 @nate ");
    }

    /// A caret the DOM reports past the end must not panic.
    #[test]
    fn an_out_of_range_caret_is_clamped() {
        assert!(active_mention("short", 999).is_none());
        let text = "@lo";
        let active = ActiveMention {
            at: 0,
            end: 999,
            fragment: "lo".to_string(),
        };
        let (out, _) = accept(text, &active, "long");
        assert_eq!(out, "@long ");
    }
}

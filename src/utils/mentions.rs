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
    if at > 0 {
        let prev = text[..at].chars().next_back().unwrap_or(' ');
        // `name@example.com` and `path/to@thing` are not mentions.
        if prev.is_alphanumeric() || prev == '.' || prev == '_' || prev == '-' {
            return None;
        }
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
}

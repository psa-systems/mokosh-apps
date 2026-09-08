//! Resolving an inline comment's anchor against the article as rendered
//! (MAPPS-744).
//!
//! The server stores a W3C `TextQuoteSelector` (PMS-1130): the quoted text,
//! up to 64 characters either side of it, and a positional hint. It never
//! reads one. Where the quote IS in today's article is decided here, on
//! every render, over the rendered text, because the body is editable
//! Markdown and any anchor pinned to a DOM position or a character offset
//! breaks on the next edit. The offsets are a hint only.
//!
//! The ladder, top rung first:
//!
//! 1. the quote occurs exactly once: that is it;
//! 2. it occurs more than once: the occurrence whose surroundings best match
//!    the stored prefix and suffix, ties going to the one nearest the hint;
//! 3. it does not occur: the closest approximate match (Sellers' algorithm,
//!    a bounded edit distance), seeded from the hint, when the distance is
//!    within [`fuzzy_budget`];
//! 4. otherwise the anchor is an **orphan**. The comment stays in the stream,
//!    labelled, quoting what it was attached to; it is never dropped and
//!    never attached to a candidate below the threshold, because a wrong
//!    passage is worse than no passage.
//!
//! Everything here works on `char` indices into the rendered text, so an
//! offset means the same thing on both hosts and in a test with no DOM.
//!
//! The server trims every string in a JSON body (PMS-924), so `prefix`,
//! `suffix` and `exact` are compared trimmed here, and `exact` is what the
//! server kept, which is what the reader selected minus its edge whitespace.

use std::ops::Range;

/// The stored selector, decoded from the comment's `anchor` JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub exact: String,
    pub prefix: String,
    pub suffix: String,
    /// The character offsets at anchoring time, when the client sent them.
    pub hint: Option<Range<usize>>,
}

/// How many characters of context are kept either side of a quote when one
/// is captured; the server refuses more (PMS-1130).
pub const CONTEXT_CHARS: usize = 64;

impl Anchor {
    /// Decode the server's JSON. `None` for a value that is not a text
    /// quote selector, which the server would not have stored.
    pub fn from_json(value: &serde_json::Value) -> Option<Anchor> {
        let exact = value.get("exact")?.as_str()?.trim().to_string();
        if exact.is_empty() {
            return None;
        }
        let text = |key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };
        let hint = value.get("refinedBy").and_then(|r| {
            let start = r.get("start")?.as_u64()? as usize;
            let end = r.get("end")?.as_u64()? as usize;
            (end >= start).then_some(start..end)
        });
        Some(Anchor {
            exact,
            prefix: text("prefix"),
            suffix: text("suffix"),
            hint,
        })
    }

    /// The JSON the server accepts, for a quote just captured.
    pub fn to_json(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "type": "TextQuoteSelector",
            "exact": self.exact,
            "prefix": self.prefix,
            "suffix": self.suffix,
        });
        if let Some(hint) = &self.hint {
            v["refinedBy"] = serde_json::json!({
                "type": "TextPositionSelector",
                "start": hint.start,
                "end": hint.end,
            });
        }
        v
    }

    /// Capture an anchor for the characters `range` of `text`: the quote,
    /// up to [`CONTEXT_CHARS`] either side, and the range as the hint.
    /// `None` for an empty or out-of-range selection.
    pub fn capture(text: &str, range: Range<usize>) -> Option<Anchor> {
        let chars: Vec<char> = text.chars().collect();
        if range.start >= range.end || range.end > chars.len() {
            return None;
        }
        let exact: String = chars[range.clone()].iter().collect::<String>();
        let exact = exact.trim().to_string();
        if exact.is_empty() {
            return None;
        }
        let prefix: String = chars[range.start.saturating_sub(CONTEXT_CHARS)..range.start]
            .iter()
            .collect();
        let suffix: String = chars[range.end..(range.end + CONTEXT_CHARS).min(chars.len())]
            .iter()
            .collect();
        Some(Anchor {
            exact,
            prefix: prefix.trim().to_string(),
            suffix: suffix.trim().to_string(),
            hint: Some(range),
        })
    }
}

/// The edit distance an approximate match may carry: a fifth of the quote,
/// at least two so a one-word quote can lose a letter, at most twenty so a
/// long quote cannot drift onto a different sentence.
pub fn fuzzy_budget(quote_chars: usize) -> usize {
    (quote_chars / 5).clamp(2, 20)
}

/// Where `anchor` is in `text`, as a `char` range, or `None` for an orphan.
pub fn resolve(text: &str, anchor: &Anchor) -> Option<Range<usize>> {
    let chars: Vec<char> = text.chars().collect();
    let quote: Vec<char> = anchor.exact.chars().collect();
    if quote.is_empty() || chars.len() < quote.len() {
        return None;
    }
    let exact_hits = find_all(&chars, &quote);
    match exact_hits.len() {
        1 => return Some(exact_hits[0]..exact_hits[0] + quote.len()),
        n if n > 1 => {
            let best = exact_hits
                .iter()
                .copied()
                .max_by_key(|&at| {
                    let score = context_score(&chars, at, quote.len(), anchor);
                    // Higher score first; on a tie, the one nearest the hint,
                    // expressed as a smaller distance being a larger key.
                    (score, usize::MAX - distance_to_hint(at, anchor))
                })
                .expect("non-empty");
            return Some(best..best + quote.len());
        }
        _ => {}
    }
    let budget = fuzzy_budget(quote.len());
    approximate_find(
        &chars,
        &quote,
        budget,
        anchor.hint.as_ref().map(|h| h.start),
    )
}

/// Every start index at which `needle` occurs in `hay`.
fn find_all(hay: &[char], needle: &[char]) -> Vec<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return Vec::new();
    }
    (0..=hay.len() - needle.len())
        .filter(|&i| hay[i..i + needle.len()] == *needle)
        .collect()
}

/// How well the text around an occurrence matches the stored context: the
/// number of matching characters walking outwards from the quote on each
/// side. Whitespace runs are folded so a re-wrapped paragraph still scores.
fn context_score(chars: &[char], at: usize, len: usize, anchor: &Anchor) -> usize {
    let before: Vec<char> = fold_ws(chars[at.saturating_sub(CONTEXT_CHARS)..at].iter().copied());
    let after: Vec<char> = fold_ws(
        chars[at + len..(at + len + CONTEXT_CHARS).min(chars.len())]
            .iter()
            .copied(),
    );
    let prefix: Vec<char> = fold_ws(anchor.prefix.chars());
    let suffix: Vec<char> = fold_ws(anchor.suffix.chars());
    let back = before
        .iter()
        .rev()
        .zip(prefix.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let forward = after
        .iter()
        .zip(suffix.iter())
        .take_while(|(a, b)| a == b)
        .count();
    back + forward
}

fn fold_ws(it: impl Iterator<Item = char>) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    for c in it {
        if c.is_whitespace() {
            if out.last().is_some_and(|l| *l == ' ') {
                continue;
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    while out.first().is_some_and(|c| *c == ' ') {
        out.remove(0);
    }
    while out.last().is_some_and(|c| *c == ' ') {
        out.pop();
    }
    out
}

fn distance_to_hint(at: usize, anchor: &Anchor) -> usize {
    match &anchor.hint {
        Some(h) => at.abs_diff(h.start),
        None => 0,
    }
}

/// Sellers' approximate substring search: the substring of `text` closest
/// to `pattern` within `max_dist` edits, ties going to the candidate whose
/// start is nearest `near`. Returns the `char` range of the match.
///
/// The forward pass finds the end of the best match; a second pass over the
/// reversed strings, anchored at that end, finds its start. O(text x
/// pattern), which for a long article against a paragraph-sized quote is a
/// few million cell updates: fine on every render of one article.
pub fn approximate_find(
    text: &[char],
    pattern: &[char],
    max_dist: usize,
    near: Option<usize>,
) -> Option<Range<usize>> {
    if pattern.is_empty() || text.is_empty() {
        return None;
    }
    // Best (distance, end) with the nearest-to-hint tie break decided after
    // the pass, so a later candidate at the same distance can win on
    // proximity.
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    let m = pattern.len();
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur: Vec<usize> = vec![0; m + 1];
    for (j, &tc) in text.iter().enumerate() {
        cur[0] = 0;
        for i in 1..=m {
            let cost = usize::from(pattern[i - 1] != tc);
            cur[i] = (prev[i - 1] + cost).min(prev[i] + 1).min(cur[i - 1] + 1);
        }
        if cur[m] <= max_dist {
            candidates.push((cur[m], j + 1));
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let best_dist = candidates.iter().map(|c| c.0).min()?;
    // Of the candidates at the best distance, an end is the last cell of a
    // run of equal distances; take the run's first end for a stable answer,
    // then the one nearest the hint.
    let ends: Vec<usize> = candidates
        .iter()
        .filter(|c| c.0 == best_dist)
        .map(|c| c.1)
        .collect();
    let mut starts_ends: Vec<Range<usize>> = ends
        .iter()
        .map(|&end| {
            let start = match_start(text, pattern, end, best_dist);
            start..end
        })
        .collect();
    starts_ends.dedup();
    let pick = match near {
        Some(n) => starts_ends
            .iter()
            .min_by_key(|r| r.start.abs_diff(n))
            .cloned(),
        None => starts_ends.first().cloned(),
    };
    pick
}

/// The start of the match ending at `end`: the same DP over the reversed
/// pattern against the text read backwards from `end`, stopping at the
/// first column whose distance is `dist`.
fn match_start(text: &[char], pattern: &[char], end: usize, dist: usize) -> usize {
    let rev_pat: Vec<char> = pattern.iter().rev().copied().collect();
    let m = rev_pat.len();
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur: Vec<usize> = vec![0; m + 1];
    let mut best = (usize::MAX, end);
    for (k, &tc) in text[..end].iter().rev().enumerate() {
        cur[0] = 0;
        for i in 1..=m {
            let cost = usize::from(rev_pat[i - 1] != tc);
            cur[i] = (prev[i - 1] + cost).min(prev[i] + 1).min(cur[i - 1] + 1);
        }
        let start = end - (k + 1);
        // Prefer the longest span at the best distance so a match does not
        // stop short of a leading character the edit budget allowed: within
        // the budget a tie goes to the later (longer) span, outside it only
        // a strictly better distance moves the answer.
        let better = if cur[m] <= dist {
            cur[m] <= best.0
        } else {
            cur[m] < best.0
        };
        if better {
            best = (cur[m], start);
        }
        if cur[m] == 0 && dist == 0 {
            break;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    best.1
}

/// A piece of highlighted text and every thread that covers it. Overlapping
/// anchors are split at every boundary so each fragment has one set of
/// owners, which is what the DOM needs: a `<mark>` cannot straddle another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment<T> {
    pub range: Range<usize>,
    pub owners: Vec<T>,
}

/// Split `ranges` at every boundary into non-overlapping fragments, each
/// carrying the ids of every range that covers it, in document order.
pub fn split_overlaps<T: Clone + PartialEq>(ranges: &[(T, Range<usize>)]) -> Vec<Fragment<T>> {
    let mut bounds: Vec<usize> = ranges.iter().flat_map(|(_, r)| [r.start, r.end]).collect();
    bounds.sort_unstable();
    bounds.dedup();
    let mut out: Vec<Fragment<T>> = Vec::new();
    for pair in bounds.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let owners: Vec<T> = ranges
            .iter()
            .filter(|(_, r)| r.start <= a && r.end >= b)
            .map(|(id, _)| id.clone())
            .collect();
        if !owners.is_empty() && a < b {
            out.push(Fragment {
                range: a..b,
                owners,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(exact: &str, prefix: &str, suffix: &str, hint: Option<Range<usize>>) -> Anchor {
        Anchor {
            exact: exact.into(),
            prefix: prefix.into(),
            suffix: suffix.into(),
            hint,
        }
    }

    fn slice(text: &str, r: &Range<usize>) -> String {
        text.chars().skip(r.start).take(r.end - r.start).collect()
    }

    #[test]
    fn a_unique_quote_resolves_to_its_one_place() {
        let text = "Restart the router. Then call the customer back.";
        let r = resolve(text, &anchor("call the customer", "", "", None)).expect("resolved");
        assert_eq!(slice(text, &r), "call the customer");
    }

    #[test]
    fn a_repeated_quote_is_placed_by_its_context_then_by_the_hint() {
        let text = "Step 1: reboot. Check the light. Step 2: reboot. Check the light again.";
        let by_ctx = resolve(text, &anchor("reboot", "Step 2:", "", None)).expect("resolved");
        assert_eq!(
            by_ctx.start,
            text.find("Step 2: reboot").unwrap() + "Step 2: ".len()
        );
        let first = text.find("reboot").unwrap();
        let by_hint =
            resolve(text, &anchor("reboot", "", "", Some(first..first + 6))).expect("resolved");
        assert_eq!(by_hint.start, first);
        // A re-wrapped paragraph still scores its context.
        let wrapped = resolve(text, &anchor("reboot", "Step   2:", "", None)).expect("resolved");
        assert_eq!(wrapped, by_ctx);
    }

    #[test]
    fn a_lightly_edited_quote_still_resolves_and_a_rewritten_one_orphans() {
        let text = "Restart the modem before you call the customer back.";
        // The quote was captured before "router" became "modem".
        let r = resolve(
            text,
            &anchor("Restart the router before", "", "", Some(0..25)),
        )
        .expect("fuzzy");
        assert_eq!(slice(text, &r), "Restart the modem before");
        // A quote with nothing left of it is an orphan, not a guess.
        assert_eq!(
            resolve(
                text,
                &anchor("Escalate to the vendor immediately", "", "", None)
            ),
            None
        );
        assert_eq!(resolve("", &anchor("x", "", "", None)), None);
    }

    #[test]
    fn the_fuzzy_budget_scales_with_the_quote_and_is_capped() {
        assert_eq!(fuzzy_budget(1), 2);
        assert_eq!(fuzzy_budget(50), 10);
        assert_eq!(fuzzy_budget(500), 20);
    }

    #[test]
    fn capture_takes_the_quote_and_trimmed_context_and_round_trips_as_json() {
        let text = "Alpha beta gamma delta epsilon.";
        let range = text.find("gamma").unwrap()..text.find("gamma").unwrap() + 5;
        let a = Anchor::capture(text, range.clone()).expect("captured");
        assert_eq!(a.exact, "gamma");
        assert_eq!(a.prefix, "Alpha beta");
        assert_eq!(a.suffix, "delta epsilon.");
        assert_eq!(a.hint, Some(range));
        let back = Anchor::from_json(&a.to_json()).expect("decoded");
        assert_eq!(back, a);
        assert_eq!(Anchor::capture(text, 3..3), None);
        assert_eq!(Anchor::capture("   ", 0..2), None);
    }

    #[test]
    fn overlapping_anchors_split_at_every_boundary_with_every_owner() {
        let ranges = vec![("a", 0..10), ("b", 5..15), ("c", 20..25)];
        let frags = split_overlaps(&ranges);
        assert_eq!(
            frags,
            vec![
                Fragment {
                    range: 0..5,
                    owners: vec!["a"]
                },
                Fragment {
                    range: 5..10,
                    owners: vec!["a", "b"]
                },
                Fragment {
                    range: 10..15,
                    owners: vec!["b"]
                },
                Fragment {
                    range: 20..25,
                    owners: vec!["c"]
                },
            ]
        );
        assert!(split_overlaps::<&str>(&[]).is_empty());
    }
}

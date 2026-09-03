//! Word-level diff of two versions of a value (MAPPS-601).
//!
//! MAPPS-596 stopped a long change-history entry dominating the pane by putting
//! it behind a `Details` toggle. Expanding one still showed the old value and
//! the new value whole, side by side, and for a description edit that is close
//! to unreadable: two strings that are identical except for a sentence
//! somewhere in the middle, both cut off at 160 characters, and the reader is
//! asked to spot the difference by eye.
//!
//! This finds the difference instead.
//!
//! ## Why not a crate
//!
//! `similar` is the obvious candidate and does far more than this needs
//! (character diffs, unified output, inline refinement), all of which lands in
//! a WASM bundle that is already 10MB. What is actually required is an LCS over
//! word tokens, which is textbook and fits in a page. It is also pure, so it is
//! tested on strings with no browser involved.
//!
//! ## Shape
//!
//! Tokens carry their trailing whitespace, so reassembling the pieces gives
//! back the original text exactly. Every function here is total: there is no
//! input for which it refuses, because it renders an audit row that has already
//! been written and cannot be corrected.

/// One run of the diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Present in both, unchanged.
    Same(String),
    /// In the old value only.
    Removed(String),
    /// In the new value only.
    Added(String),
}

impl Piece {
    /// The text this run carries.
    pub fn text(&self) -> &str {
        match self {
            Piece::Same(s) | Piece::Removed(s) | Piece::Added(s) => s,
        }
    }
}

/// Above this many tokens on either side, the quadratic table is not worth
/// building and the values are reported as a wholesale replacement.
///
/// Reached only after the shared prefix and suffix have been trimmed, which for
/// a real edit leaves a handful of tokens however long the values are. What
/// survives to this cap is two values with almost nothing in common, where a
/// word diff would be noise anyway: a description replaced outright, or one
/// language swapped for another.
const MAX_TOKENS: usize = 400;

/// Fraction of the longer value that has to survive as unchanged text before a
/// diff is worth showing at all.
///
/// "Open" to "Closed" shares the letter O and nothing else meaningful; splitting
/// it into fragments would be worse than saying one replaced the other, which is
/// how every short field has always rendered. A description edit is far above
/// this.
const MIN_SHARED: f32 = 0.3;

/// Split into word tokens, each carrying the whitespace that followed it.
///
/// Whitespace rides along rather than becoming a token of its own so that
/// concatenating every token reproduces the input, and so a diff never reports
/// a changed space.
fn tokenize(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (idx, ch) in s.char_indices() {
        if ch.is_whitespace() {
            in_space = true;
        } else if in_space {
            // The first non-space after whitespace closes the previous token,
            // which therefore ends with the whitespace that followed its word.
            out.push(&s[start..idx]);
            start = idx;
            in_space = false;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Diff `old` against `new` at word granularity.
///
/// Returns `None` when the two share too little for a diff to be more readable
/// than saying one replaced the other; the caller then renders them whole, the
/// way every change-history entry did before this.
pub fn diff_words(old: &str, new: &str) -> Option<Vec<Piece>> {
    if old == new {
        return Some(vec![Piece::Same(old.to_string())]);
    }
    if old.is_empty() || new.is_empty() {
        return None;
    }

    let a = tokenize(old);
    let b = tokenize(new);

    // Shared prefix and suffix first. A one-sentence edit inside a long
    // description reduces to a handful of tokens here, which is what keeps the
    // table below small for the case this feature exists for.
    let mut head = 0;
    while head < a.len() && head < b.len() && a[head] == b[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < a.len() - head
        && tail < b.len() - head
        && a[a.len() - 1 - tail] == b[b.len() - 1 - tail]
    {
        tail += 1;
    }

    let mid_a = &a[head..a.len() - tail];
    let mid_b = &b[head..b.len() - tail];

    if mid_a.len() > MAX_TOKENS || mid_b.len() > MAX_TOKENS {
        return None;
    }

    let mut pieces = Vec::new();
    push_run(&mut pieces, Piece::Same(a[..head].concat()));
    lcs_into(mid_a, mid_b, &mut pieces);
    push_run(&mut pieces, Piece::Same(a[a.len() - tail..].concat()));

    let shared: usize = pieces
        .iter()
        .filter(|p| matches!(p, Piece::Same(_)))
        .map(|p| p.text().chars().count())
        .sum();
    let longest = old.chars().count().max(new.chars().count());
    if longest == 0 || (shared as f32) / (longest as f32) < MIN_SHARED {
        return None;
    }

    Some(pieces)
}

/// Append, merging into the previous run when it is the same kind. Keeps the
/// output free of adjacent runs that would render as two spans.
fn push_run(out: &mut Vec<Piece>, piece: Piece) {
    if piece.text().is_empty() {
        return;
    }
    match (out.last_mut(), &piece) {
        (Some(Piece::Same(prev)), Piece::Same(s))
        | (Some(Piece::Removed(prev)), Piece::Removed(s))
        | (Some(Piece::Added(prev)), Piece::Added(s)) => prev.push_str(s),
        _ => out.push(piece),
    }
}

/// Longest common subsequence over the two token slices, emitted as runs.
fn lcs_into(a: &[&str], b: &[&str], out: &mut Vec<Piece>) {
    if a.is_empty() && b.is_empty() {
        return;
    }
    if a.is_empty() {
        push_run(out, Piece::Added(b.concat()));
        return;
    }
    if b.is_empty() {
        push_run(out, Piece::Removed(a.concat()));
        return;
    }

    // table[i][j] = length of the LCS of a[i..] and b[j..].
    let (n, m) = (a.len(), b.len());
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[at(i, j)] = if a[i] == b[j] {
                table[at(i + 1, j + 1)] + 1
            } else {
                table[at(i + 1, j)].max(table[at(i, j + 1)])
            };
        }
    }

    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            push_run(out, Piece::Same(a[i].to_string()));
            i += 1;
            j += 1;
        } else if table[at(i + 1, j)] >= table[at(i, j + 1)] {
            push_run(out, Piece::Removed(a[i].to_string()));
            i += 1;
        } else {
            push_run(out, Piece::Added(b[j].to_string()));
            j += 1;
        }
    }
    if i < n {
        push_run(out, Piece::Removed(a[i..].concat()));
    }
    if j < m {
        push_run(out, Piece::Added(b[j..].concat()));
    }
}

/// Shorten long unchanged runs to `context` words at each edge, so a one-word
/// edit in a long description does not print the description twice.
///
/// An elided run keeps a `…` in place of what it dropped, so the reader can see
/// that there is more rather than being shown a fragment that looks whole.
pub fn elide(pieces: Vec<Piece>, context: usize) -> Vec<Piece> {
    let last = pieces.len().saturating_sub(1);
    let mut out = Vec::with_capacity(pieces.len());
    for (idx, piece) in pieces.into_iter().enumerate() {
        let Piece::Same(text) = &piece else {
            out.push(piece);
            continue;
        };
        let words = tokenize(text);
        // A leading run only needs its end, a trailing run only its start, and
        // one in the middle needs both.
        let (keep_head, keep_tail) = match (idx == 0, idx == last) {
            (true, true) => (words.len(), 0),
            (true, false) => (0, context),
            (false, true) => (context, 0),
            (false, false) => (context, context),
        };
        if words.len() <= keep_head + keep_tail {
            out.push(piece);
            continue;
        }
        let mut shortened = String::new();
        if keep_head > 0 {
            shortened.push_str(&words[..keep_head].concat());
        } else {
            shortened.push('…');
            shortened.push(' ');
        }
        if keep_head > 0 && keep_tail > 0 {
            shortened.push_str("… ");
        }
        if keep_tail > 0 {
            shortened.push_str(&words[words.len() - keep_tail..].concat());
        } else if keep_head > 0 {
            shortened.push('…');
        }
        out.push(Piece::Same(shortened));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(s: &str) -> Piece {
        Piece::Same(s.to_string())
    }
    fn removed(s: &str) -> Piece {
        Piece::Removed(s.to_string())
    }
    fn added(s: &str) -> Piece {
        Piece::Added(s.to_string())
    }

    /// Reassembling the old side (everything but the additions) has to give back
    /// the old value exactly, and likewise the new. Without that the diff is
    /// showing the reader something neither version ever said.
    fn assert_round_trips(old: &str, new: &str, pieces: &[Piece]) {
        let rebuilt_old: String = pieces
            .iter()
            .filter(|p| !matches!(p, Piece::Added(_)))
            .map(Piece::text)
            .collect();
        let rebuilt_new: String = pieces
            .iter()
            .filter(|p| !matches!(p, Piece::Removed(_)))
            .map(Piece::text)
            .collect();
        assert_eq!(rebuilt_old, old, "the removals rebuild the old value");
        assert_eq!(rebuilt_new, new, "the additions rebuild the new value");
    }

    #[test]
    fn an_edit_in_the_middle_is_the_only_thing_marked() {
        let old = "The domain expired and then reactivated.";
        let new = "The domain expired and was then reactivated.";
        let pieces = diff_words(old, new).expect("these share almost everything");
        assert_eq!(
            pieces,
            vec![
                same("The domain expired and "),
                added("was "),
                same("then reactivated."),
            ]
        );
        assert_round_trips(old, new, &pieces);
    }

    #[test]
    fn a_replaced_word_is_a_removal_next_to_an_addition() {
        let old = "All DNS entries are gone.";
        let new = "All MX entries are gone.";
        let pieces = diff_words(old, new).unwrap();
        assert_eq!(
            pieces,
            vec![
                same("All "),
                removed("DNS "),
                added("MX "),
                same("entries are gone."),
            ]
        );
        assert_round_trips(old, new, &pieces);
    }

    /// The reporter's case: two long values differing by one clause. What the
    /// reader needs is the clause, not both values.
    #[test]
    fn a_long_description_reduces_to_the_clause_that_changed() {
        let old = "Rachel's email is not working. The domain expired and then reactivated. \
                   It appears all DNS entries are gone. Contact the registrar for the zone file.";
        let new = "Rachel's email is not working. The domain expired and then reactivated. \
                   It appears all DNS entries are gone. Contact Google for the zone file.";
        let pieces = diff_words(old, new).unwrap();
        assert_round_trips(old, new, &pieces);
        let changed: Vec<&Piece> = pieces
            .iter()
            .filter(|p| !matches!(p, Piece::Same(_)))
            .collect();
        assert_eq!(changed.len(), 2, "one removal and one addition: {pieces:?}");
        assert_eq!(changed[0], &removed("the registrar "));
        assert_eq!(changed[1], &added("Google "));
    }

    /// A short field is a replacement, not an edit. Fragmenting "Open" and
    /// "Closed" into shared letters would be worse than what shipped before.
    #[test]
    fn two_unrelated_short_values_are_not_worth_diffing() {
        assert_eq!(diff_words("Open", "Closed"), None);
        assert_eq!(diff_words("Low", "High"), None);
        assert_eq!(diff_words("Normal", "Urgent"), None);
    }

    /// An added or cleared value has no counterpart to diff against.
    #[test]
    fn an_empty_side_is_not_a_diff() {
        assert_eq!(diff_words("", "something"), None);
        assert_eq!(diff_words("something", ""), None);
    }

    /// Two values with nothing in common are a replacement however long they
    /// are, and saying so is more readable than interleaving them.
    #[test]
    fn a_wholesale_rewrite_is_reported_as_one() {
        let old = "alpha bravo charlie delta echo foxtrot";
        let new = "one two three four five six seven";
        assert_eq!(diff_words(old, new), None);
    }

    /// Whitespace rides with its token, so a diff never reports a changed space
    /// and the pieces reassemble byte for byte. Newlines included: a Markdown
    /// description is full of them.
    #[test]
    fn whitespace_survives_the_round_trip() {
        let old = "line one\n\n- alpha\n- bravo\n";
        let new = "line one\n\n- alpha\n- charlie\n";
        let pieces = diff_words(old, new).unwrap();
        assert_round_trips(old, new, &pieces);
    }

    /// Multi-byte characters must not be split. The tokenizer walks char
    /// boundaries, and this fails loudly if it ever stops.
    #[test]
    fn multibyte_text_is_not_cut_mid_character() {
        let old = "Le café est fermé aujourd'hui.";
        let new = "Le café est ouvert aujourd'hui.";
        let pieces = diff_words(old, new).unwrap();
        assert_round_trips(old, new, &pieces);
    }

    #[test]
    fn an_unchanged_value_is_one_unchanged_run() {
        assert_eq!(
            diff_words("same text", "same text"),
            Some(vec![same("same text")])
        );
    }

    /// Elision keeps the reader oriented without printing the whole document:
    /// a few words either side of the change and a marker for the rest.
    #[test]
    fn a_long_unchanged_run_between_two_changes_is_elided() {
        let filler = "word ".repeat(40);
        let pieces = vec![
            removed("before "),
            Piece::Same(filler.clone()),
            added("after"),
        ];
        let out = elide(pieces, 3);
        let Piece::Same(middle) = &out[1] else {
            panic!("the middle run stays a Same run: {out:?}");
        };
        assert!(middle.contains('…'), "and says something was dropped");
        assert!(
            middle.split_whitespace().count() < 10,
            "three words a side plus the marker, not forty: {middle:?}"
        );
        assert!(middle.starts_with("word word word"));
        assert!(middle.ends_with("word word word "));
    }

    /// A leading run only needs its END: what precedes a change matters, what
    /// starts the document does not.
    #[test]
    fn a_leading_run_keeps_the_words_next_to_the_change() {
        let pieces = vec![
            Piece::Same("one two three four five six ".into()),
            added("new"),
        ];
        let out = elide(pieces, 2);
        let Piece::Same(head) = &out[0] else {
            panic!("{out:?}");
        };
        assert!(head.starts_with('…'), "the start is dropped: {head:?}");
        assert!(head.ends_with("five six "), "the end is kept: {head:?}");
    }

    /// A single unchanged run IS the whole value, so there is nothing to orient
    /// the reader against and nothing to elide.
    #[test]
    fn an_unchanged_value_is_never_elided() {
        let pieces = vec![Piece::Same("word ".repeat(40))];
        assert_eq!(elide(pieces.clone(), 3), pieces);
    }

    /// The table is quadratic, so a pathological pair has to bail rather than
    /// build it. Reached only after the shared prefix and suffix are trimmed,
    /// which is why a genuine edit never gets here however long the values are.
    #[test]
    fn two_enormous_unrelated_values_bail_before_the_table() {
        let old: String = (0..MAX_TOKENS + 50).map(|i| format!("a{i} ")).collect();
        let new: String = (0..MAX_TOKENS + 50).map(|i| format!("b{i} ")).collect();
        assert_eq!(diff_words(&old, &new), None);
    }

    /// And a genuine edit inside two enormous values does NOT bail, because the
    /// prefix and suffix trim leaves almost nothing for the table.
    #[test]
    fn a_small_edit_in_two_enormous_values_still_diffs() {
        let head: String = (0..MAX_TOKENS + 50).map(|i| format!("w{i} ")).collect();
        let old = format!("{head}alpha {head}");
        let new = format!("{head}bravo {head}");
        let pieces = diff_words(&old, &new).expect("the trim leaves two tokens");
        assert_round_trips(&old, &new, &pieces);
        let changed: Vec<&Piece> = pieces
            .iter()
            .filter(|p| !matches!(p, Piece::Same(_)))
            .collect();
        assert_eq!(changed, vec![&removed("alpha "), &added("bravo ")]);
    }
}

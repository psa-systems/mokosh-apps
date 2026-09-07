//! Line-level diff of two versions of an article body (MAPPS-739).
//!
//! The KB keeps every version of an article and lets a manager restore any
//! of them, and until this the only way to know what a restore would do was
//! to read two bodies side by side. This says what changed: which lines went,
//! which came, and inside a changed pair of lines which words moved.
//!
//! ## Why lines, and why here
//!
//! The body is Markdown, and Markdown is written in lines: a paragraph, a
//! bullet, a heading, a fence row. A line is the unit the author edited and
//! the unit the reader can scan, so it is the unit of the diff. Word-level
//! refinement inside a changed line comes from [`word_diff::diff_words`],
//! which already exists for the change-history feed, and the LCS table is the
//! one that module runs over words, so nothing is written twice and no diff
//! crate joins the bundle (the `similar` decision recorded on `word_diff`).
//!
//! Computed here rather than on the server because every version body is
//! already on the client (the history card fetches all of them), and because
//! the diff a restore shows has to agree with what the restore lands, which
//! is exactly "the current body against version n".
//!
//! Every function is total: two bodies always produce a diff, and a pair too
//! large for the table is reported as a wholesale replacement rather than
//! refused.

use crate::utils::word_diff::{diff_words, lcs_into, Piece};

/// What became of one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Same,
    Removed,
    Added,
}

/// One line of the diff, without its trailing newline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
    /// For a removed line that was rewritten as the added line that follows
    /// (and for that added line), the word-level pieces of the rewrite: a
    /// removed line keeps its `Same` and `Removed` pieces, an added line its
    /// `Same` and `Added` ones. `None` when the line was not part of such a
    /// pair or the pair shares too little for words to help.
    pub pieces: Option<Vec<Piece>>,
}

/// The whole diff plus the two numbers the history card prints.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LineDiff {
    pub lines: Vec<DiffLine>,
    pub added: usize,
    pub removed: usize,
}

impl LineDiff {
    /// True when nothing changed.
    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }
}

/// Above this many lines on either side after the shared prefix and suffix
/// are trimmed, the table is not built and the middle is reported as a
/// wholesale replacement. A real edit of a long article trims to a handful
/// of lines; what survives to this cap is two bodies with almost nothing in
/// common.
const MAX_LINES: usize = 1200;

/// Split into lines that keep their `\n`, so concatenating the tokens gives
/// the input back and a run boundary is always a line boundary.
fn lines_of(s: &str) -> Vec<&str> {
    s.split_inclusive('\n').collect()
}

/// Diff `old` against `new` line by line, refining each rewritten line pair
/// with the word diff.
pub fn diff_lines(old: &str, new: &str) -> LineDiff {
    if old == new {
        let lines = lines_of(old)
            .into_iter()
            .map(|l| DiffLine {
                kind: LineKind::Same,
                text: strip_newline(l).to_string(),
                pieces: None,
            })
            .collect();
        return LineDiff {
            lines,
            added: 0,
            removed: 0,
        };
    }

    let a = lines_of(old);
    let b = lines_of(new);

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

    let mut runs: Vec<Piece> = Vec::new();
    if !a[..head].is_empty() {
        runs.push(Piece::Same(a[..head].concat()));
    }
    if mid_a.len() > MAX_LINES || mid_b.len() > MAX_LINES {
        if !mid_a.is_empty() {
            runs.push(Piece::Removed(mid_a.concat()));
        }
        if !mid_b.is_empty() {
            runs.push(Piece::Added(mid_b.concat()));
        }
    } else {
        lcs_into(mid_a, mid_b, &mut runs);
    }
    if tail > 0 {
        runs.push(Piece::Same(a[a.len() - tail..].concat()));
    }

    // Runs back to lines. A run's text is whole tokens, so splitting on the
    // newline each token carries gives the tokens back.
    let mut lines: Vec<DiffLine> = Vec::new();
    for run in &runs {
        let (kind, text) = match run {
            Piece::Same(t) => (LineKind::Same, t),
            Piece::Removed(t) => (LineKind::Removed, t),
            Piece::Added(t) => (LineKind::Added, t),
        };
        for l in lines_of(text) {
            lines.push(DiffLine {
                kind,
                text: strip_newline(l).to_string(),
                pieces: None,
            });
        }
    }

    refine_pairs(&mut lines);

    let added = lines.iter().filter(|l| l.kind == LineKind::Added).count();
    let removed = lines.iter().filter(|l| l.kind == LineKind::Removed).count();
    LineDiff {
        lines,
        added,
        removed,
    }
}

fn strip_newline(l: &str) -> &str {
    l.strip_suffix('\n').unwrap_or(l)
}

/// Where a block of removed lines is immediately followed by a block of
/// added lines, the two were one rewrite: pair them up in order and word-diff
/// each pair, so "one word changed in a long paragraph" reads as that word
/// rather than as the paragraph twice.
fn refine_pairs(lines: &mut [DiffLine]) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != LineKind::Removed {
            i += 1;
            continue;
        }
        let removed_start = i;
        while i < lines.len() && lines[i].kind == LineKind::Removed {
            i += 1;
        }
        let added_start = i;
        while i < lines.len() && lines[i].kind == LineKind::Added {
            i += 1;
        }
        let pairs = (added_start - removed_start).min(i - added_start);
        for k in 0..pairs {
            let (r, a) = (removed_start + k, added_start + k);
            let Some(pieces) = diff_words(&lines[r].text, &lines[a].text) else {
                continue;
            };
            lines[r].pieces = Some(
                pieces
                    .iter()
                    .filter(|p| !matches!(p, Piece::Added(_)))
                    .cloned()
                    .collect(),
            );
            lines[a].pieces = Some(
                pieces
                    .into_iter()
                    .filter(|p| !matches!(p, Piece::Removed(_)))
                    .collect(),
            );
        }
    }
}

/// A row of the rendered diff: a line, or a count of unchanged lines that
/// were folded away between two changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Line(DiffLine),
    Folded(usize),
}

/// Keep `context` unchanged lines on each side of every change and fold the
/// rest, so a one-line edit in a long article does not print the article.
/// A diff with no changes is returned whole: there is nothing to fold
/// around, and "everything folded" would read as an empty diff.
pub fn fold(diff: &LineDiff, context: usize) -> Vec<Row> {
    if diff.is_empty() {
        return diff.lines.iter().cloned().map(Row::Line).collect();
    }
    let n = diff.lines.len();
    let mut keep = vec![false; n];
    for (idx, line) in diff.lines.iter().enumerate() {
        if line.kind != LineKind::Same {
            let lo = idx.saturating_sub(context);
            let hi = (idx + context + 1).min(n);
            for k in keep.iter_mut().take(hi).skip(lo) {
                *k = true;
            }
        }
    }
    let mut out = Vec::new();
    let mut folded = 0usize;
    for (idx, line) in diff.lines.iter().enumerate() {
        if keep[idx] {
            if folded > 0 {
                out.push(Row::Folded(folded));
                folded = 0;
            }
            out.push(Row::Line(line.clone()));
        } else {
            folded += 1;
        }
    }
    if folded > 0 {
        out.push(Row::Folded(folded));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(diff: &LineDiff) -> Vec<(LineKind, &str)> {
        diff.lines
            .iter()
            .map(|l| (l.kind, l.text.as_str()))
            .collect()
    }

    #[test]
    fn identical_bodies_diff_to_nothing() {
        let body = "# Title\n\nOne.\nTwo.\n";
        let d = diff_lines(body, body);
        assert!(d.is_empty());
        assert_eq!(d.lines.len(), 4);
        assert!(d.lines.iter().all(|l| l.kind == LineKind::Same));
    }

    #[test]
    fn an_inserted_line_is_one_addition_in_place() {
        let old = "a\nb\nd\n";
        let new = "a\nb\nc\nd\n";
        let d = diff_lines(old, new);
        assert_eq!((d.added, d.removed), (1, 0));
        assert_eq!(
            kinds(&d),
            vec![
                (LineKind::Same, "a"),
                (LineKind::Same, "b"),
                (LineKind::Added, "c"),
                (LineKind::Same, "d"),
            ]
        );
    }

    #[test]
    fn a_deleted_line_is_one_removal_in_place() {
        let d = diff_lines("a\nb\nc\n", "a\nc\n");
        assert_eq!((d.added, d.removed), (0, 1));
        assert_eq!(
            d.lines[1],
            DiffLine {
                kind: LineKind::Removed,
                text: "b".into(),
                pieces: None
            }
        );
    }

    #[test]
    fn a_rewritten_line_is_a_pair_refined_to_its_words() {
        let old = "Restart the router before you call.\n";
        let new = "Restart the modem before you call.\n";
        let d = diff_lines(old, new);
        assert_eq!((d.added, d.removed), (1, 1));
        let removed = &d.lines[0];
        let added = &d.lines[1];
        assert_eq!(removed.kind, LineKind::Removed);
        assert_eq!(added.kind, LineKind::Added);
        let removed_pieces = removed.pieces.as_ref().expect("removed line refined");
        assert!(removed_pieces.contains(&Piece::Removed("router ".into())));
        assert!(!removed_pieces.iter().any(|p| matches!(p, Piece::Added(_))));
        let added_pieces = added.pieces.as_ref().expect("added line refined");
        assert!(added_pieces.contains(&Piece::Added("modem ".into())));
        assert!(!added_pieces.iter().any(|p| matches!(p, Piece::Removed(_))));
    }

    #[test]
    fn a_wholesale_rewrite_is_not_refined() {
        let d = diff_lines("Open\n", "Closed for good\n");
        assert!(d.lines.iter().all(|l| l.pieces.is_none()));
        assert_eq!((d.added, d.removed), (1, 1));
    }

    #[test]
    fn a_moved_block_counts_on_both_sides() {
        let old = "intro\n\nsteps\n1\n2\n\nnotes\nn\n";
        let new = "intro\n\nnotes\nn\n\nsteps\n1\n2\n";
        let d = diff_lines(old, new);
        // Lines are the same set, so the diff is the smaller block moved:
        // the LCS keeps one block in place and reports the other twice.
        assert_eq!(d.added, d.removed);
        assert!(d.added > 0);
        assert!(d
            .lines
            .iter()
            .any(|l| l.kind == LineKind::Same && l.text == "intro"));
    }

    #[test]
    fn a_missing_trailing_newline_is_not_a_change_in_the_text() {
        let d = diff_lines("a\nb", "a\nb\n");
        // The last token differs ("b" vs "b\n"), which is a real byte
        // change, but the lines it prints are both "b".
        assert!(d.lines.iter().all(|l| l.text != "b\n"));
    }

    #[test]
    fn a_pair_beyond_the_table_is_a_wholesale_replacement() {
        let old: String = (0..MAX_LINES + 5).map(|i| format!("old {i}\n")).collect();
        let new: String = (0..MAX_LINES + 5).map(|i| format!("new {i}\n")).collect();
        let d = diff_lines(&old, &new);
        assert_eq!((d.added, d.removed), (MAX_LINES + 5, MAX_LINES + 5));
    }

    #[test]
    fn folding_keeps_context_around_each_change_and_counts_the_rest() {
        let old: String = (0..20).map(|i| format!("l{i}\n")).collect();
        let new = old.replace("l10\n", "l10 changed\n");
        let d = diff_lines(&old, &new);
        let rows = fold(&d, 2);
        // 8 folded, l8 l9, -l10 +l10', l11 l12, 7 folded.
        assert_eq!(rows.first(), Some(&Row::Folded(8)));
        assert_eq!(rows.last(), Some(&Row::Folded(7)));
        let shown: Vec<&str> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Line(l) => Some(l.text.as_str()),
                Row::Folded(_) => None,
            })
            .collect();
        assert_eq!(shown, ["l8", "l9", "l10", "l10 changed", "l11", "l12"]);
    }

    #[test]
    fn an_unchanged_diff_is_not_folded_away() {
        let d = diff_lines("a\nb\n", "a\nb\n");
        assert_eq!(fold(&d, 1).len(), 2);
    }
}

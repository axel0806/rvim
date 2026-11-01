// SPDX-License-Identifier: Apache-2.0
//! Three-way line merge for the conflict prompt's `m` key (PLAN.md, "Agent
//! experience", "Conflict prompt"): base is the last text known to be on disk,
//! `mine` is the buffer, `theirs` is the file now. This module is pure: no file
//! IO and no terminal.
//!
//! Each side is compared with the base by a line diff (LCS). The changed
//! regions of the two sides are grouped when they overlap or touch. A group
//! changed on one side only takes that side's text. A group that both sides
//! changed the same way takes that text once. Any other group is a conflict,
//! written into the text as
//!
//! ```text
//! <<<<<<< mine
//! ...the buffer's lines...
//! =======
//! ...the file's lines...
//! >>>>>>> disk
//! ```
//!
//! Only conflicting groups get markers. Unchanged lines are copied as they are.

use std::ops::Range;

/// The marker lines of a conflict, as written by [`merge3`].
pub(crate) const MARK_MINE: &str = "<<<<<<< mine";
pub(crate) const MARK_SEP: &str = "=======";
pub(crate) const MARK_THEIRS: &str = ">>>>>>> disk";

/// Cap on the LCS table (cells) of one side's diff. Above it the side is one
/// replacement of the whole base, so the result is a conflict over the whole
/// text unless both sides agree (conservative, never a lost line).
pub(crate) const MAX_LCS_CELLS: usize = 4_000_000;

/// The result of a merge: the merged lines and the number of conflicts.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Merged {
    pub lines: Vec<String>,
    pub conflicts: usize,
}

/// One changed region of a side: base lines `start..end` become `new`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Hunk {
    start: usize,
    end: usize,
    new: Vec<String>,
}

/// Merges `mine` and `theirs`, both changed from `base`.
pub(crate) fn merge3(base: &[String], mine: &[String], theirs: &[String]) -> Merged {
    let hm = hunks(base, mine);
    let ht = hunks(base, theirs);
    let mut out = Vec::new();
    let mut conflicts = 0;
    let mut pos = 0;
    for group in groups(&hm, &ht) {
        out.extend_from_slice(&base[pos..group.span.start]);
        let mine_text = rebuild(base, group.span.clone(), &group.mine);
        let theirs_text = rebuild(base, group.span.clone(), &group.theirs);
        if group.mine.is_empty() {
            out.extend(theirs_text);
        } else if group.theirs.is_empty() || mine_text == theirs_text {
            // Changed on one side only, or the same way on both: no conflict.
            out.extend(mine_text);
        } else {
            conflicts += 1;
            out.push(MARK_MINE.to_string());
            out.extend(mine_text);
            out.push(MARK_SEP.to_string());
            out.extend(theirs_text);
            out.push(MARK_THEIRS.to_string());
        }
        pos = group.span.end;
    }
    out.extend_from_slice(&base[pos..]);
    Merged {
        lines: out,
        conflicts,
    }
}

/// A group of overlapping or touching changes from both sides.
struct Group {
    span: Range<usize>,
    mine: Vec<Hunk>,
    theirs: Vec<Hunk>,
}

/// Sorts the changes of both sides into groups. A change joins the group
/// while it starts at or before the group's end, so changes that touch are
/// one group (as in Git, where adjacent changes conflict).
fn groups(mine: &[Hunk], theirs: &[Hunk]) -> Vec<Group> {
    let mut all: Vec<(bool, &Hunk)> = mine
        .iter()
        .map(|h| (true, h))
        .chain(theirs.iter().map(|h| (false, h)))
        .collect();
    all.sort_by_key(|(_, h)| (h.start, h.end));
    let mut out: Vec<Group> = Vec::new();
    for (is_mine, h) in all {
        let joins = out.last().is_some_and(|g| h.start <= g.span.end);
        if !joins {
            out.push(Group {
                span: h.start..h.end,
                mine: Vec::new(),
                theirs: Vec::new(),
            });
        }
        let g = out.last_mut().expect("a group was just pushed or joined");
        g.span.end = g.span.end.max(h.end);
        if is_mine {
            g.mine.push(h.clone());
        } else {
            g.theirs.push(h.clone());
        }
    }
    out
}

/// The base lines of `span` with one side's changes applied. `hunks` are that
/// side's changes inside `span`, in order, and they do not overlap.
fn rebuild(base: &[String], span: Range<usize>, hunks: &[Hunk]) -> Vec<String> {
    let mut out = Vec::new();
    let mut p = span.start;
    for h in hunks {
        out.extend_from_slice(&base[p..h.start]);
        out.extend(h.new.iter().cloned());
        p = h.end;
    }
    out.extend_from_slice(&base[p..span.end]);
    out
}

/// The changed regions of `side` against `base`, in order.
fn hunks(base: &[String], side: &[String]) -> Vec<Hunk> {
    // Common head and tail are never changes.
    let head = base.iter().zip(side).take_while(|(a, b)| a == b).count();
    let tail = base[head..]
        .iter()
        .rev()
        .zip(side[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let a = &base[head..base.len() - tail];
    let b = &side[head..side.len() - tail];
    if (a.len() + 1).saturating_mul(b.len() + 1) > MAX_LCS_CELLS {
        return vec![Hunk {
            start: 0,
            end: base.len(),
            new: side.to_vec(),
        }];
    }
    let lcs = lcs_suffixes(a, b);
    let cols = b.len() + 1;
    let at = |i: usize, j: usize| lcs[i * cols + j];
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut cur: Option<Hunk> = None;
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            out.extend(cur.take());
            i += 1;
            j += 1;
            continue;
        }
        let h = cur.get_or_insert_with(|| Hunk {
            start: head + i,
            end: head + i,
            new: Vec::new(),
        });
        if j < b.len() && (i == a.len() || at(i, j + 1) >= at(i + 1, j)) {
            h.new.push(b[j].clone());
            j += 1;
        } else {
            h.end = head + i + 1;
            i += 1;
        }
    }
    out.extend(cur.take());
    out
}

/// `lcs[i * (b.len() + 1) + j]` is the length of the longest common
/// subsequence of `a[i..]` and `b[j..]`.
fn lcs_suffixes(a: &[String], b: &[String]) -> Vec<u32> {
    let cols = b.len() + 1;
    let mut t = vec![0u32; (a.len() + 1) * cols];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            t[i * cols + j] = if a[i] == b[j] {
                t[(i + 1) * cols + j + 1] + 1
            } else {
                t[(i + 1) * cols + j].max(t[i * cols + j + 1])
            };
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clean_merge_takes_both_sides_changes() {
        let base = v(&["one", "two", "three", "four", "five"]);
        let mine = v(&["ONE", "two", "three", "four", "five"]);
        let theirs = v(&["one", "two", "three", "four", "FIVE"]);
        let m = merge3(&base, &mine, &theirs);
        assert_eq!(m.lines, v(&["ONE", "two", "three", "four", "FIVE"]));
        assert_eq!(m.conflicts, 0);
    }

    #[test]
    fn conflicting_change_to_the_same_line_is_marked() {
        let base = v(&["a", "b", "c"]);
        let mine = v(&["a", "MINE", "c"]);
        let theirs = v(&["a", "DISK", "c"]);
        let m = merge3(&base, &mine, &theirs);
        assert_eq!(
            m.lines,
            v(&[
                "a",
                "<<<<<<< mine",
                "MINE",
                "=======",
                "DISK",
                ">>>>>>> disk",
                "c"
            ])
        );
        assert_eq!(m.conflicts, 1);
    }

    #[test]
    fn identical_changes_on_both_sides_are_not_a_conflict() {
        let base = v(&["a", "b", "c"]);
        let same = v(&["a", "X", "c"]);
        let m = merge3(&base, &same, &same);
        assert_eq!(m.lines, same);
        assert_eq!(m.conflicts, 0);
    }

    #[test]
    fn one_side_unchanged_takes_the_other_side() {
        let base = v(&["a", "b", "c"]);
        let mine = v(&["a", "b", "c", "d"]);
        let m = merge3(&base, &mine, &base);
        assert_eq!(m.lines, mine);
        assert_eq!(m.conflicts, 0);
        let m = merge3(&base, &base, &mine);
        assert_eq!(m.lines, mine);
        assert_eq!(m.conflicts, 0);
    }

    #[test]
    fn touching_changes_conflict_as_in_git() {
        // Mine changes line 2, theirs inserts after line 1: adjacent, so one
        // conflicting group.
        let base = v(&["a", "b"]);
        let mine = v(&["a", "B"]);
        let theirs = v(&["a", "new", "b"]);
        let m = merge3(&base, &mine, &theirs);
        assert_eq!(m.conflicts, 1);
        assert!(m.lines.contains(&MARK_SEP.to_string()));
    }

    #[test]
    fn both_sides_insert_at_the_start_of_an_empty_base() {
        let base: Vec<String> = Vec::new();
        let m = merge3(&base, &v(&["x"]), &v(&["y"]));
        assert_eq!(m.conflicts, 1);
        let m = merge3(&base, &v(&["x"]), &v(&["x"]));
        assert_eq!(m.lines, v(&["x"]));
        assert_eq!(m.conflicts, 0);
    }

    #[test]
    fn deletion_on_one_side_and_edit_on_the_other_conflict() {
        let base = v(&["a", "b", "c"]);
        let mine = v(&["a", "c"]);
        let theirs = v(&["a", "B", "c"]);
        let m = merge3(&base, &mine, &theirs);
        assert_eq!(m.conflicts, 1);
        assert_eq!(
            m.lines,
            v(&["a", "<<<<<<< mine", "=======", "B", ">>>>>>> disk", "c"])
        );
    }

    #[test]
    fn disjoint_changes_with_an_unchanged_line_between_are_clean() {
        let base = v(&["a", "b", "c", "d", "e"]);
        let mine = v(&["A", "b", "c", "d", "e"]);
        let theirs = v(&["a", "b", "c", "d", "E"]);
        let m = merge3(&base, &mine, &theirs);
        assert_eq!(m.lines, v(&["A", "b", "c", "d", "E"]));
        assert_eq!(m.conflicts, 0);
    }

    #[test]
    fn no_changes_gives_the_base() {
        let base = v(&["a", "b"]);
        let m = merge3(&base, &base, &base);
        assert_eq!(m.lines, base);
        assert_eq!(m.conflicts, 0);
    }
}

// SPDX-License-Identifier: Apache-2.0
//! Reload of a file that changed on disk (PLAN.md, "Agent experience: how it
//! fits together", "Reload as one undo step"). This module is pure: no file
//! IO and no terminal. It holds the disk state kept per document, a line
//! diff between the buffer and the new text, and the mapping of the cursor
//! and the scroll top through that diff. The editor applies the result
//! (`Editor::apply_external_change`).
//!
//! The mapping rules, where PLAN.md is explicit and where it was not:
//! - A line whose text is equal in both texts keeps its place in the diff:
//!   the cursor and the scroll top move with it, and its column is kept.
//! - A line inside a run of deleted or replaced lines goes to the first new
//!   line of the replacement. A pure deletion goes to the line before the
//!   deletion (PLAN). A pure deletion at the top of the file, with no line
//!   before it, goes to the first line after it (not in PLAN: the simplest
//!   rule, and the one that keeps the cursor on the text that remains).
//! - Such a line takes the desired column, clipped (PLAN).
//! - When the scroll top itself is not equal in both texts, the view keeps the
//!   cursor on the row it had: the top is moved by the same number of lines
//!   as the cursor (not in PLAN: "as close as possible", made concrete).
//! - Above `MAX_DIFF_LINES` lines, `MAX_DIFF_BYTES` bytes, or a middle part
//!   of `MAX_DP_CELLS` cells, there is no diff: every line keeps its line
//!   number, clamped to the new text, and the message says so.

use crate::motion::Pos;
use std::time::SystemTime;

/// Above this many lines in either text, no line diff is made.
pub(crate) const MAX_DIFF_LINES: usize = 20_000;
/// Above this many bytes in either text, no line diff is made.
pub(crate) const MAX_DIFF_BYTES: usize = 4 << 20;
/// Cap on the size of the LCS table (cells) for the part between the common
/// first and last lines. A table of u32 is 16 MB at this size.
pub(crate) const MAX_DP_CELLS: usize = 4_000_000;

/// FNV-1a, 64 bit: a small, stable, non-cryptographic hash. The same bytes
/// give the same value in every run, so a recorded hash stays valid.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |h, &b| (h ^ u64::from(b)).wrapping_mul(PRIME))
}

/// The state of a file on disk as the buffer last read or wrote it (Vim's
/// `b_mtime` and friends, plus the hash for echo suppression).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiskState {
    pub mtime: Option<SystemTime>,
    pub size: usize,
    /// `fnv1a` of the file's bytes, not of the text (the line ends count).
    pub hash: u64,
}

impl DiskState {
    /// The state of a file with modification time `mtime` and contents
    /// `bytes`.
    pub(crate) fn of(mtime: Option<SystemTime>, bytes: &[u8]) -> Self {
        Self {
            mtime,
            size: bytes.len(),
            hash: fnv1a(bytes),
        }
    }

    /// Whether `now` is the same file state: same modification time, size
    /// and contents.
    pub(crate) fn same_as(&self, now: &DiskState) -> bool {
        self.mtime == now.mtime && self.size == now.size && self.hash == now.hash
    }
}

/// Why the file is not in step with the buffer (PLAN.md, "Conflict prompt"
/// and the deleted-file row). Set by `:checktime`, cleared by a reload or a
/// write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stale {
    Changed,
    Deleted,
}

/// The message `:checktime` and the watcher last showed for a file, so the
/// watcher shows a warning once per change (Document::warned).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Warned {
    /// A change to the file with this hash.
    Changed(u64),
    /// The file is gone.
    Deleted,
}

/// What `:checktime` and the file watcher do about a file (one decision for
/// both, so they cannot disagree). `synced` is the state the buffer last read
/// or wrote, None when it has no file on disk yet; `now` is the file's state
/// now, None when it is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// The file is as the buffer last saw it, or there is no file either time.
    Same,
    /// Same contents, a new modification time or size: record it, no change.
    Touched,
    /// The file was created after editing started (W13).
    Created,
    /// The file is gone (E211). The buffer is kept.
    Deleted,
    /// The file changed and the buffer is kept: it is modified (W12), or
    /// 'autoread' is off (W11).
    Changed { modified: bool },
    /// The file changed, the buffer is unmodified and 'autoread' is on: reload.
    Reload,
}

/// The decision for a file, given the state the buffer synced with, the
/// file's state now, whether the buffer is modified, and 'autoread'.
pub(crate) fn external_change_decision(
    synced: Option<&DiskState>,
    now: Option<&DiskState>,
    modified: bool,
    autoread: bool,
) -> Decision {
    match (synced, now) {
        (None, None) => Decision::Same,
        (Some(_), None) => Decision::Deleted,
        (None, Some(_)) => Decision::Created,
        (Some(d), Some(n)) if d.same_as(n) => Decision::Same,
        (Some(d), Some(n)) if d.hash == n.hash => Decision::Touched,
        (Some(_), Some(_)) if modified || !autoread => Decision::Changed { modified },
        (Some(_), Some(_)) => Decision::Reload,
    }
}

/// What a reload changed, for the message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Change {
    /// Lines of the new text that are not in the old text.
    pub added: usize,
    /// Lines of the old text that are not in the new text.
    pub removed: usize,
    /// No line diff was made (too large): the cursor kept its line number.
    pub fallback: bool,
}

/// The line correspondence between an old and a new text, and where a line
/// of the old text goes in the new one.
#[derive(Clone, Debug)]
pub(crate) struct LineMap {
    /// For each old line: the new line it goes to, and whether its text is
    /// equal (`true`) or it was deleted or replaced (`false`). Empty when
    /// there is no diff.
    targets: Vec<(usize, bool)>,
    /// Character count of each new line.
    new_lens: Vec<usize>,
    /// The lines before `top` are the same in both texts, and so are the
    /// lines after the `old_count` old lines that start at `top`.
    pub top: usize,
    pub old_count: usize,
    pub added: usize,
    pub removed: usize,
    pub fallback: bool,
}

impl LineMap {
    /// The map from `old` to `new`, which are the lines of the two texts
    /// (without line ends). Both texts have at least one line in the editor;
    /// an empty slice is treated as one empty line.
    pub(crate) fn new<S: AsRef<str>>(old: &[S], new: &[S]) -> Self {
        let new_lens: Vec<usize> = new.iter().map(|s| s.as_ref().chars().count()).collect();
        let (n, m) = (old.len(), new.len());
        // Common first and last lines: always equal, no diff needed.
        let pre = old
            .iter()
            .zip(new)
            .take_while(|(a, b)| a.as_ref() == b.as_ref())
            .count();
        let suf = old[pre..]
            .iter()
            .rev()
            .zip(new[pre..].iter().rev())
            .take_while(|(a, b)| a.as_ref() == b.as_ref())
            .count();
        let top = pre;
        let old_count = n - pre - suf;
        let last = m.saturating_sub(1);
        match matched_pairs(old, new, pre, suf) {
            Some(pairs) => {
                let mut targets = vec![(0usize, false); n];
                let (mut i, mut j) = (0, 0);
                for &(mi, mj) in pairs.iter().chain(std::iter::once(&(n, m))) {
                    // Lines between two matches: new lines `j..mj` replace
                    // old lines `i..mi`.
                    for t in &mut targets[i..mi] {
                        *t = (
                            if mj > j { j } else { j.saturating_sub(1) }.min(last),
                            false,
                        );
                    }
                    if mi < n {
                        targets[mi] = (mj, true);
                    }
                    i = mi + 1;
                    j = mj + 1;
                }
                Self {
                    targets,
                    new_lens,
                    top,
                    old_count,
                    added: m - pairs.len(),
                    removed: n - pairs.len(),
                    fallback: false,
                }
            }
            None => Self {
                targets: Vec::new(),
                new_lens,
                top,
                old_count,
                added: 0,
                removed: 0,
                fallback: true,
            },
        }
    }

    /// The new line of old line `line`, and whether its text is unchanged.
    /// A line outside the old text goes to the last new line.
    pub(crate) fn line(&self, line: usize) -> (usize, bool) {
        let last = self.new_lens.len().saturating_sub(1);
        match self.targets.get(line) {
            Some(&t) => t,
            None => (line.min(last), false),
        }
    }

    /// The new position of the old position `p`. The column is `p.col` when
    /// the line is unchanged, otherwise `desired_col`; either way it is
    /// clipped to the last character of the new line (Normal mode).
    pub(crate) fn pos(&self, p: Pos, desired_col: usize) -> Pos {
        let (line, exact) = self.line(p.line);
        let len = self.new_lens.get(line).copied().unwrap_or(0);
        let col = if exact { p.col } else { desired_col };
        Pos::new(line, col.min(len.saturating_sub(1)))
    }

    /// The new scroll top, given the old top and the cursor's old and new
    /// lines. An unchanged top line maps like any line. A changed one keeps
    /// the cursor on the same row of the window.
    pub(crate) fn top(
        &self,
        old_top: usize,
        old_cursor_line: usize,
        new_cursor_line: usize,
    ) -> usize {
        let last = self.new_lens.len().saturating_sub(1);
        let (line, exact) = self.line(old_top);
        if exact {
            return line;
        }
        let offset = old_cursor_line.saturating_sub(old_top);
        new_cursor_line.saturating_sub(offset).min(last)
    }
}

/// The new position of `cursor` (old text to new text), with the column
/// rule of `LineMap::pos` and a desired column equal to the cursor column.
/// Pure helper for the rules above; the editor uses `LineMap` directly.
#[cfg(test)]
pub(crate) fn map_cursor<S: AsRef<str>>(old: &[S], new: &[S], cursor: Pos) -> Pos {
    LineMap::new(old, new).pos(cursor, cursor.col)
}

/// The pairs `(old, new)` of equal lines, in order, for the whole texts: the
/// common first lines, an LCS of the part between them, and the common last
/// lines. `None` when a text is over the size caps.
fn matched_pairs<S: AsRef<str>>(
    old: &[S],
    new: &[S],
    pre: usize,
    suf: usize,
) -> Option<Vec<(usize, usize)>> {
    let bytes = |t: &[S]| t.iter().map(|s| s.as_ref().len()).sum::<usize>();
    if old.len() > MAX_DIFF_LINES
        || new.len() > MAX_DIFF_LINES
        || bytes(old) > MAX_DIFF_BYTES
        || bytes(new) > MAX_DIFF_BYTES
    {
        return None;
    }
    let (n, m) = (old.len(), new.len());
    let mut pairs: Vec<(usize, usize)> = (0..pre).map(|i| (i, i)).collect();
    let a = &old[pre..n - suf];
    let b = &new[pre..m - suf];
    if !a.is_empty() && !b.is_empty() {
        if a.len().saturating_mul(b.len()) > MAX_DP_CELLS {
            return None;
        }
        // t[i][j]: the LCS length of a[i..] and b[j..].
        let w = b.len() + 1;
        let mut t = vec![0u32; (a.len() + 1) * w];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                t[i * w + j] = if a[i].as_ref() == b[j].as_ref() {
                    t[(i + 1) * w + j + 1] + 1
                } else {
                    t[(i + 1) * w + j].max(t[i * w + j + 1])
                };
            }
        }
        // Walking forward, a equal pair is always part of some LCS.
        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            if a[i].as_ref() == b[j].as_ref() {
                pairs.push((pre + i, pre + j));
                i += 1;
                j += 1;
            } else if t[(i + 1) * w + j] >= t[i * w + j + 1] {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    pairs.extend((0..suf).map(|k| (n - suf + k, m - suf + k)));
    Some(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<&str> {
        s.split('\n').collect()
    }

    #[test]
    fn fnv1a_matches_the_reference_values() {
        // Reference values of FNV-1a 64 (the empty string is the offset basis).
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn disk_state_hashes_the_bytes() {
        let a = DiskState::of(None, b"x\r\n");
        let b = DiskState::of(None, b"x\n");
        assert_ne!(a.hash, b.hash, "line ends are part of the hash");
        assert_eq!(a.size, 3);
        assert!(a.same_as(&DiskState::of(None, b"x\r\n")));
        assert!(!a.same_as(&b));
    }

    #[test]
    fn decision_table_for_checktime_and_the_watcher() {
        use Decision::*;
        let a = DiskState::of(None, b"a\n");
        let b = DiskState::of(None, b"bb\n");
        let a_touched = DiskState::of(Some(SystemTime::UNIX_EPOCH), b"a\n");
        let d = external_change_decision;
        assert_eq!(d(None, None, false, true), Same);
        assert_eq!(d(Some(&a), None, false, true), Deleted);
        assert_eq!(d(None, Some(&a), false, true), Created);
        assert_eq!(d(Some(&a), Some(&a), true, true), Same);
        assert_eq!(d(Some(&a), Some(&a_touched), false, false), Touched);
        assert_eq!(d(Some(&a), Some(&b), false, true), Reload);
        assert_eq!(
            d(Some(&a), Some(&b), false, false),
            Changed { modified: false }
        );
        assert_eq!(
            d(Some(&a), Some(&b), true, true),
            Changed { modified: true }
        );
    }

    // The mapping table of PLAN.md, one row per case.

    #[test]
    fn equal_text_keeps_the_cursor() {
        let t = lines("one\ntwo\nthree");
        assert_eq!(map_cursor(&t, &t, Pos::new(1, 2)), Pos::new(1, 2));
        let map = LineMap::new(&t, &t);
        assert_eq!((map.added, map.removed, map.fallback), (0, 0, false));
    }

    #[test]
    fn lines_inserted_above_shift_the_cursor() {
        let old = lines("one\ntwo\nthree");
        let new = lines("zero\nzero2\none\ntwo\nthree");
        // The cursor was on "two"; it is on "two" again, two lines lower.
        assert_eq!(map_cursor(&old, &new, Pos::new(1, 1)), Pos::new(3, 1));
        assert_eq!(map_cursor(&old, &new, Pos::new(2, 0)), Pos::new(4, 0));
    }

    #[test]
    fn line_under_the_cursor_deleted_goes_to_the_line_before() {
        let old = lines("one\ntwo\nthree");
        let new = lines("one\nthree");
        assert_eq!(map_cursor(&old, &new, Pos::new(1, 2)), Pos::new(0, 2));
        // Lines after the deletion still follow their text.
        assert_eq!(map_cursor(&old, &new, Pos::new(2, 0)), Pos::new(1, 0));
    }

    #[test]
    fn line_under_the_cursor_replaced_goes_to_the_first_new_line_with_desired_column() {
        let old = lines("one\ntwo\nthree");
        let new = lines("one\nTWO-A\nTWO-B\nthree");
        let map = LineMap::new(&old, &new);
        // Desired column 3 on the new line "TWO-A": kept; 9 is clipped to 4.
        assert_eq!(map.pos(Pos::new(1, 0), 3), Pos::new(1, 3));
        assert_eq!(map.pos(Pos::new(1, 0), 9), Pos::new(1, 4));
        // Desired column past the end of the new line is clipped.
        let short = lines("one\nx\nthree");
        let map = LineMap::new(&old, &short);
        assert_eq!(map.pos(Pos::new(1, 0), 9), Pos::new(1, 0));
        assert_eq!(map_cursor(&old, &new, Pos::new(1, 2)), Pos::new(1, 2));
        assert_eq!(map.added, 1);
        assert_eq!(map.removed, 1);
    }

    #[test]
    fn deleted_top_line_goes_to_the_first_line_after_it() {
        let old = lines("one\ntwo\nthree");
        let new = lines("two\nthree");
        assert_eq!(map_cursor(&old, &new, Pos::new(0, 1)), Pos::new(0, 1));
        assert_eq!(LineMap::new(&old, &new).line(0), (0, false));
        // Its old line 1 is still "two", now the first line.
        assert_eq!(LineMap::new(&old, &new).line(1), (0, true));
    }

    #[test]
    fn deleted_run_at_the_end_goes_to_the_last_line() {
        let old = lines("one\ntwo\nthree");
        let new = lines("one");
        assert_eq!(map_cursor(&old, &new, Pos::new(2, 0)), Pos::new(0, 0));
    }

    #[test]
    fn a_whole_new_text_clamps_to_its_last_line() {
        let old = lines("a\nb\nc");
        let new = lines("x");
        assert_eq!(map_cursor(&old, &new, Pos::new(2, 0)), Pos::new(0, 0));
    }

    #[test]
    fn scroll_top_maps_like_the_cursor_when_its_line_is_equal() {
        let old = lines("a\nb\nc\nd\ne");
        let new = lines("X\nb\nc\nd\ne");
        let map = LineMap::new(&old, &new);
        // Top "b" (line 1) is equal: it stays the top.
        assert_eq!(map.top(1, 3, 3), 1);
        // Insert above: top "b" moves down with its text.
        let ins = lines("X\nY\na\nb\nc\nd\ne");
        let map = LineMap::new(&old, &ins);
        assert_eq!(map.top(1, 3, 5), 3);
    }

    #[test]
    fn scroll_top_on_a_changed_line_keeps_the_cursor_row() {
        let old = lines("a\nb\nc\nd\ne\nf");
        // "a" is replaced by two lines. The cursor was on "d" (line 3), three
        // rows below the top.
        let new = lines("A1\nA2\nb\nc\nd\ne\nf");
        let map = LineMap::new(&old, &new);
        assert!(!map.line(0).1);
        // "d" is now line 4, so the top is line 1, three rows above it.
        assert_eq!(map.top(0, 3, 4), 1);
    }

    #[test]
    fn scroll_top_on_a_changed_line_is_not_below_the_text() {
        let old = lines("a\nb\nc");
        let new = lines("x");
        assert_eq!(LineMap::new(&old, &new).top(2, 2, 0), 0);
    }

    // Diff size cap.

    #[test]
    fn above_the_line_cap_there_is_no_diff() {
        let old: Vec<String> = (0..MAX_DIFF_LINES + 1).map(|i| i.to_string()).collect();
        let mut new = old.clone();
        new.insert(0, "new top".to_string());
        let map = LineMap::new(&old, &new);
        assert!(map.fallback);
        // The cursor keeps its line number (clamped), not its text.
        assert_eq!(map.pos(Pos::new(10, 0), 0).line, 10);
        // The top keeps the cursor's row: the cursor moved from 10 to 10,
        // five rows below the old top 5.
        assert_eq!(map.top(5, 10, 10), 5);
    }

    #[test]
    fn above_the_byte_cap_there_is_no_diff() {
        let big = "x".repeat(MAX_DIFF_BYTES + 1);
        let old = vec![big.clone(), "b".to_string()];
        let new = vec![big, "c".to_string()];
        assert!(LineMap::new(&old, &new).fallback);
    }

    #[test]
    fn a_middle_part_over_the_table_cap_is_a_fallback() {
        // Every line differs: the LCS table would be 2100 x 2100 cells, and
        // the cap is 4 million cells (above 2000 x 2000).
        let old: Vec<String> = (0..2100).map(|i| format!("o{i}")).collect();
        let new: Vec<String> = (0..2100).map(|i| format!("n{i}")).collect();
        assert!(LineMap::new(&old, &new).fallback);
        // A smaller one is diffed.
        let old: Vec<String> = (0..100).map(|i| format!("o{i}")).collect();
        let new: Vec<String> = (0..100).map(|i| format!("n{i}")).collect();
        let map = LineMap::new(&old, &new);
        assert!(!map.fallback);
        assert_eq!((map.added, map.removed), (100, 100));
    }

    #[test]
    fn the_diff_finds_the_longest_match_in_the_middle() {
        let old = lines("a\nx\nb\nc\ny\nd");
        let new = lines("a\nb\nq\nc\nd");
        let map = LineMap::new(&old, &new);
        assert_eq!(map.line(2), (1, true), "b stays b");
        assert_eq!(map.line(3), (3, true), "c stays c");
        assert_eq!(map.line(5), (4, true), "d stays d");
        assert_eq!((map.added, map.removed), (1, 2));
        assert_eq!((map.top, map.old_count), (1, 4));
    }

    #[test]
    fn empty_texts_are_one_empty_line() {
        let old: [&str; 1] = [""];
        let new = lines("a");
        let map = LineMap::new(&old, &new);
        assert_eq!(map.line(0), (0, false));
        assert!(!map.fallback);
    }
}

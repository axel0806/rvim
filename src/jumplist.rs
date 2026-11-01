//! The jumplist and the previous context mark (`''`), ports of Vim's
//! `setpcmark()`, `checkpcmark()`, `cleanup_jumplist()` and `movemark()` in
//! mark.c. In Vim these belong to the window, not the buffer, so they live
//! apart from `Buffer`: the editor hands them the line edits the buffer
//! logged (`PosEdit`), in order, before it reads or sets a position.

use crate::buffer::PosEdit;
use crate::motion::Pos;

/// Vim's JUMPLISTSIZE: the oldest entry is dropped beyond this.
pub const JUMPLIST_SIZE: usize = 100;

#[derive(Clone, Debug, Default)]
pub struct JumpList {
    entries: Vec<Pos>,
    /// The entry `<C-o>` and `<Tab>` count from. Equal to `entries.len()`
    /// when the list is not being walked.
    idx: usize,
    /// The previous context mark, `None` when it is not set.
    pcmark: Option<Pos>,
    /// The context mark before the last `set_pcmark()`, which
    /// `check_pcmark()` restores when the cursor did not move.
    prev_pcmark: Option<Pos>,
}

/// Where a line goes when `old` lines at `first` are replaced and the lines
/// after them move by `after`. `None` if the line was one of the replaced
/// ones (Vim's `one_adjust()`).
fn moved_line(line: usize, first: usize, old: usize, after: isize) -> Option<usize> {
    if line >= first + old {
        line.checked_add_signed(after)
    } else if line >= first {
        None
    } else {
        Some(line)
    }
}

impl JumpList {
    /// The previous context mark (`''`).
    pub fn pcmark(&self) -> Option<Pos> {
        self.pcmark
    }

    /// The entries oldest first, and the index `<C-o>` counts from.
    pub fn entries(&self) -> (&[Pos], usize) {
        (&self.entries, self.idx)
    }

    /// Vim's setpcmark(): `cursor` becomes the previous context mark and is
    /// added to the jumplist. The caller decides whether this is allowed
    /// (Vim does nothing inside `:global`).
    pub fn set_pcmark(&mut self, cursor: Pos) {
        self.prev_pcmark = self.pcmark;
        self.pcmark = Some(cursor);
        self.entries.push(cursor);
        if self.entries.len() > JUMPLIST_SIZE {
            self.entries.remove(0);
        }
        self.idx = self.entries.len();
    }

    /// The context mark set by `m'` and ``m` `` stays even if the cursor does
    /// not move (Vim's setmark_pos()).
    pub fn keep_pcmark(&mut self) {
        self.prev_pcmark = self.pcmark;
    }

    /// Vim's checkpcmark(), run after each command: when the cursor is back
    /// on the context mark (or the mark is gone), the mark from before the
    /// last `set_pcmark()` comes back.
    pub fn check_pcmark(&mut self, cursor: Pos) {
        if let Some(prev) = self.prev_pcmark.take()
            && (self.pcmark.is_none() || self.pcmark == Some(cursor))
        {
            self.pcmark = Some(prev);
        }
    }

    /// Vim's cleanup_jumplist(): an entry is dropped when a later entry is on
    /// the same line, so the list keeps the most recent visit of each line.
    pub fn cleanup(&mut self) {
        let mut kept: Vec<Pos> = Vec::with_capacity(self.entries.len());
        let mut idx = self.idx;
        for (from, &p) in self.entries.iter().enumerate() {
            if self.idx == from {
                idx = kept.len();
            }
            if !self.entries[from + 1..].iter().any(|q| q.line == p.line) {
                kept.push(p);
            }
        }
        if self.idx == self.entries.len() {
            idx = kept.len();
        }
        self.entries = kept;
        self.idx = idx;
    }

    /// Vim's movemark(): walks `count` entries (negative is older) from the
    /// current one and returns the entry reached, or `None` when there is no
    /// such entry. Walking from the newest end first records `cursor` as a
    /// jump, unless `record` is false (inside `:global`).
    pub fn move_by(&mut self, count: isize, cursor: Pos, record: bool) -> Option<Pos> {
        self.cleanup();
        if self.entries.is_empty() {
            return None;
        }
        let len = self.entries.len() as isize;
        if self.idx as isize + count < 0 || self.idx as isize + count >= len {
            return None;
        }
        if self.idx == self.entries.len() {
            if record {
                self.set_pcmark(cursor);
            }
            // The new entry is skipped: the walk starts from the one before.
            self.idx -= 1;
            if (self.idx as isize) + count < 0 {
                return None;
            }
        }
        self.idx = (self.idx as isize + count) as usize;
        Some(self.entries[self.idx])
    }

    /// Follows one line edit of the buffer (Vim's mark_adjust() and
    /// mark_col_adjust() for these positions). Entries on deleted lines go to
    /// the line where the deletion was; the context mark is unset instead.
    pub fn apply(&mut self, edit: &PosEdit) {
        match edit {
            PosEdit::Lines { first, old, after } => {
                let (first, old, after) = (*first, *old, *after);
                let move_pos = |p: Pos| -> Option<Pos> {
                    Some(Pos::new(moved_line(p.line, first, old, after)?, p.col))
                };
                self.pcmark = self.pcmark.and_then(move_pos);
                self.prev_pcmark = self.prev_pcmark.and_then(move_pos);
                for p in &mut self.entries {
                    *p = match moved_line(p.line, first, old, after) {
                        Some(line) => Pos::new(line, p.col),
                        // Vim's one_adjust_nodel(): the entry goes to the
                        // first deleted line, in column 0.
                        None => Pos::new(first, 0),
                    };
                }
            }
            PosEdit::Remap(f) => {
                self.pcmark = self.pcmark.map(f);
                self.prev_pcmark = self.prev_pcmark.map(f);
                for p in &mut self.entries {
                    *p = f(*p);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(line: usize) -> Pos {
        Pos::new(line, 0)
    }

    #[test]
    fn set_pcmark_records_and_moves_the_context_mark() {
        let mut j = JumpList::default();
        j.set_pcmark(p(3));
        j.set_pcmark(p(7));
        assert_eq!(j.pcmark(), Some(p(7)));
        assert_eq!(j.entries(), (&[p(3), p(7)][..], 2));
    }

    #[test]
    fn list_is_capped_at_100_entries() {
        let mut j = JumpList::default();
        for l in 0..150 {
            j.set_pcmark(p(l));
        }
        let (e, idx) = j.entries();
        assert_eq!(e.len(), JUMPLIST_SIZE);
        assert_eq!(e[0], p(50));
        assert_eq!(idx, JUMPLIST_SIZE);
    }

    #[test]
    fn cleanup_keeps_the_latest_visit_of_a_line() {
        let mut j = JumpList::default();
        for l in [1, 2, 1, 3] {
            j.set_pcmark(p(l));
        }
        j.cleanup();
        assert_eq!(j.entries(), (&[p(2), p(1), p(3)][..], 3));
    }

    #[test]
    fn move_by_walks_back_and_forward() {
        let mut j = JumpList::default();
        j.set_pcmark(p(1));
        j.set_pcmark(p(2));
        // From the newest end, the cursor (line 9) is recorded first.
        assert_eq!(j.move_by(-1, p(9), true), Some(p(2)));
        assert_eq!(j.move_by(-1, p(2), true), Some(p(1)));
        assert_eq!(j.move_by(-1, p(1), true), None);
        assert_eq!(j.move_by(1, p(1), true), Some(p(2)));
        assert_eq!(j.move_by(1, p(2), true), Some(p(9)));
        assert_eq!(j.move_by(1, p(9), true), None);
    }

    #[test]
    fn check_pcmark_restores_the_mark_when_the_cursor_did_not_move() {
        let mut j = JumpList::default();
        j.set_pcmark(p(1));
        j.check_pcmark(p(5));
        assert_eq!(j.pcmark(), Some(p(1)));

        j.set_pcmark(p(5));
        j.check_pcmark(p(5));
        assert_eq!(j.pcmark(), Some(p(1)));

        j.keep_pcmark();
        j.check_pcmark(p(1));
        assert_eq!(j.pcmark(), Some(p(1)));
    }

    #[test]
    fn deleted_lines_move_entries_and_unset_the_mark() {
        let mut j = JumpList::default();
        j.set_pcmark(p(2));
        j.set_pcmark(p(6));
        // Lines 4..=5 deleted: the entry on line 6 moves up by two, the one
        // on line 2 is unchanged.
        j.apply(&PosEdit::Lines {
            first: 4,
            old: 2,
            after: -2,
        });
        assert_eq!(j.entries().0, &[p(2), p(4)]);
        assert_eq!(j.pcmark(), Some(p(4)));

        // Deleting the line of the context mark unsets it; the jumplist
        // entry stays where the deletion was.
        j.apply(&PosEdit::Lines {
            first: 2,
            old: 1,
            after: -1,
        });
        assert_eq!(j.pcmark(), Some(p(3)));
        j.apply(&PosEdit::Lines {
            first: 3,
            old: 1,
            after: -1,
        });
        assert_eq!(j.pcmark(), None);
        assert_eq!(j.entries().0, &[p(2), p(3)]);
    }

    #[test]
    fn inserted_lines_shift_entries_below() {
        let mut j = JumpList::default();
        j.set_pcmark(p(3));
        j.apply(&PosEdit::Lines {
            first: 3,
            old: 0,
            after: 1,
        });
        assert_eq!(j.entries().0, &[p(4)]);
        assert_eq!(j.pcmark(), Some(p(4)));
    }
}

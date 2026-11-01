// SPDX-License-Identifier: Vim
// Derived in part from Vim (https://www.vim.org); see LICENSE-VIM.
// This is a modified version, not the official Vim.
//! Text storage. A buffer is a list of lines held in a rope; lines are joined
//! by `\n` and the file's final newline is not stored (Vim's 'eol' model).
//! Positions are (line, char) pairs, both zero-based.

use crate::motion::Pos;
use ropey::{Rope, RopeSlice};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The lowercase marks `a` to `z` (Vim's `b_namedm`). A mark that was
/// deleted with its line is `None`.
pub type Marks = [Option<Pos>; 26];

/// Index in `Marks` of a lowercase mark name.
fn mark_index(c: char) -> Option<usize> {
    c.is_ascii_lowercase().then(|| c as usize - 'a' as usize)
}

/// A change of line numbers, recorded for the positions the editor keeps
/// outside the buffer: the jumplist and the context mark, which belong to
/// the window (see `jumplist.rs`). The buffer logs every such edit in order
/// and the editor applies the log before it reads or sets those positions.
pub enum PosEdit {
    /// Vim's mark_adjust() for lines `first..first + old` being replaced by
    /// nothing or by other lines (undo): positions on them move to `first`
    /// (the context mark is unset), positions from `first + old` on move by
    /// `after`. An insertion has `old == 0`.
    Lines {
        first: usize,
        old: usize,
        after: isize,
    },
    /// Positions move to a new place computed from the position: the joins
    /// of `J` and `:s`, and `:m`.
    Remap(Box<dyn Fn(Pos) -> Pos>),
}

pub struct Buffer {
    rope: Rope,
    pub path: Option<PathBuf>,
    pub modified: bool,
    /// Lines end in CRLF on disk ('fileformat' dos).
    pub crlf: bool,
    /// The buffer has no lines at all (new or empty file), which Vim writes
    /// as a zero-byte file. Cleared by any edit.
    no_lines: bool,
    /// Bumped on every change, like Vim's b:changedtick.
    pub changedtick: u64,
    /// Line numbers followed through edits (for `:g`); `usize::MAX` once
    /// the line was deleted.
    pub tracked: Vec<usize>,
    /// Lowercase marks, followed through edits by the methods below.
    pub(crate) marks: Marks,
    /// Line edits not yet taken by the editor (see `PosEdit`).
    pos_edits: Vec<PosEdit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStats {
    pub lines: usize,
    pub bytes: usize,
    /// The file did not end with a newline.
    pub noeol: bool,
}

/// The text at one point in time, for undo. Cheap to clone (ropes share
/// structure).
#[derive(Clone)]
pub struct Snapshot {
    rope: Rope,
    no_lines: bool,
}

impl Snapshot {
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Line `l` without its line break.
    pub fn line(&self, l: usize) -> RopeSlice<'_> {
        line_of(&self.rope, l)
    }
}

fn line_of(rope: &Rope, l: usize) -> RopeSlice<'_> {
    let s = rope.line(l);
    if l + 1 < rope.len_lines() {
        s.slice(..s.len_chars() - 1)
    } else {
        s
    }
}

/// The bytes of file `path` and its modification time, or `None` when it does
/// not exist. The modification time is read before the contents, so a write
/// made during the read shows up as a change later rather than being missed.
pub(crate) fn read_file(path: &Path) -> io::Result<Option<(Option<SystemTime>, Vec<u8>)>> {
    let mtime = match fs::metadata(path) {
        Ok(m) => m.modified().ok(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    match fs::read(path) {
        Ok(bytes) => Ok(Some((mtime, bytes))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// File contents after decoding: the text without its final newline, CRLF
/// line ends removed when every line has one, and the file's statistics.
struct Decoded {
    text: String,
    crlf: bool,
    /// The file had no bytes at all.
    no_lines: bool,
    stats: FileStats,
}

fn decode(bytes: &[u8]) -> Decoded {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    let newlines = text.matches('\n').count();
    let crlf = newlines > 0 && text.matches("\r\n").count() == newlines;
    if crlf {
        text = text.replace("\r\n", "\n");
    }
    let noeol = !text.is_empty() && !text.ends_with('\n');
    let stats = FileStats {
        lines: newlines + usize::from(noeol),
        bytes: bytes.len(),
        noeol,
    };
    let no_lines = text.is_empty();
    if text.ends_with('\n') {
        text.pop();
    }
    Decoded {
        text,
        crlf,
        no_lines,
        stats,
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            modified: false,
            crlf: false,
            no_lines: true,
            changedtick: 0,
            tracked: Vec::new(),
            marks: [None; 26],
            pos_edits: Vec::new(),
        }
    }

    /// The position of lowercase mark `c`, if it is set.
    pub fn mark(&self, c: char) -> Option<Pos> {
        self.marks[mark_index(c)?]
    }

    /// Sets lowercase mark `c`. Setting a mark is not a change.
    pub fn set_mark(&mut self, c: char, pos: Pos) {
        if let Some(i) = mark_index(c) {
            self.marks[i] = Some(pos);
        }
    }

    /// Unsets lowercase mark `c` (`:delmarks`).
    pub(crate) fn set_mark_unset(&mut self, c: char) {
        if let Some(i) = mark_index(c) {
            self.marks[i] = None;
        }
    }

    /// Unsets all lowercase marks (`:delmarks!`).
    pub(crate) fn clear_marks(&mut self) {
        self.marks = [None; 26];
    }

    /// Vim's `mark_adjust()` for the lowercase marks: lines `first..first +
    /// old` were replaced by `new` lines. Marks on the replaced lines are
    /// deleted, marks below them move by the difference, marks above stay.
    pub(crate) fn adjust_marks(&mut self, first: usize, old: usize, new: usize) {
        let delta = new as isize - old as isize;
        self.pos_edits.push(PosEdit::Lines {
            first,
            old,
            after: delta,
        });
        for slot in self.marks.iter_mut() {
            let Some(p) = slot.as_mut() else { continue };
            if p.line >= first + old {
                p.line = (p.line as isize + delta) as usize;
            } else if p.line >= first {
                *slot = None;
            }
        }
    }

    /// Removes the marks on lines `first..=last` and returns them, to be set
    /// again where those lines end up.
    pub(crate) fn take_marks_in(&mut self, first: usize, last: usize) -> Vec<(char, Pos)> {
        let mut out = Vec::new();
        for (i, slot) in self.marks.iter_mut().enumerate() {
            if let Some(p) = *slot
                && (first..=last).contains(&p.line)
            {
                out.push((char::from(b'a' + i as u8), p));
                *slot = None;
            }
        }
        out
    }

    /// Moves the marks on line `from` to line `to`, mapping their columns
    /// with `col` (used when lines are joined). The jumplist and the context
    /// mark are moved the same way.
    pub(crate) fn move_marks_to_line(
        &mut self,
        from: usize,
        to: usize,
        col: impl Fn(usize) -> usize + Clone + 'static,
    ) {
        for p in self.marks.iter_mut().flatten() {
            if p.line == from {
                p.line = to;
                p.col = col(p.col);
            }
        }
        let col = col.clone();
        self.pos_edits.push(PosEdit::Remap(Box::new(move |p: Pos| {
            if p.line == from {
                Pos::new(to, col(p.col))
            } else {
                p
            }
        })));
    }

    /// The number of line edits logged so far; see `replace_pos_edits_from`.
    pub(crate) fn pos_edit_count(&self) -> usize {
        self.pos_edits.len()
    }

    /// Replaces the line edits logged since `mark` with `edit`, for a change
    /// whose effect on the positions outside the buffer is computed in one
    /// piece (`:m`, and `:s` over several lines).
    pub(crate) fn replace_pos_edits_from(&mut self, mark: usize, edit: PosEdit) {
        self.pos_edits.truncate(mark);
        self.pos_edits.push(edit);
    }

    /// Takes the line edits logged so far, for the editor to apply.
    pub(crate) fn take_pos_edits(&mut self) -> Vec<PosEdit> {
        std::mem::take(&mut self.pos_edits)
    }

    /// Lines separated by `\n`; `"a\nb"` is two lines.
    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            no_lines: text.is_empty(),
            ..Self::new()
        }
    }

    /// Loads `path`. A missing file gives an empty buffer with that name and
    /// `None` stats (Vim's "[New]").
    pub fn load(path: &Path) -> io::Result<(Self, Option<FileStats>)> {
        let read = read_file(path)?;
        Ok(Self::from_file_bytes(
            path,
            read.as_ref().map(|(_, b)| b.as_slice()),
        ))
    }

    /// The buffer for file `path` whose contents are `bytes` (`None` for a
    /// missing file), decoded as `load` does.
    pub(crate) fn from_file_bytes(path: &Path, bytes: Option<&[u8]>) -> (Self, Option<FileStats>) {
        let Some(bytes) = bytes else {
            let buf = Self {
                path: Some(path.to_owned()),
                ..Self::new()
            };
            return (buf, None);
        };
        let d = decode(bytes);
        let buf = Self {
            rope: Rope::from_str(&d.text),
            path: Some(path.to_owned()),
            crlf: d.crlf,
            no_lines: d.no_lines,
            ..Self::new()
        };
        (buf, Some(d.stats))
    }

    /// Sets the line-end style and the empty state of a reloaded file whose
    /// text is the same as the buffer's. Not a change of text, so no undo.
    pub(crate) fn set_file_flags(&mut self, crlf: bool, no_lines: bool) {
        self.crlf = crlf;
        self.no_lines = no_lines;
    }

    /// Replaces the whole text with the text of `new` (a file changed on
    /// disk, read by `from_file_bytes`), including its line-end style and
    /// its empty state. The position log gets `remap`, which moves the
    /// positions outside the buffer (the jumplist, the context mark) to the
    /// new lines. Marks are not touched here; the caller maps them. The
    /// buffer is modified afterwards; the caller clears that when the text
    /// is the file's.
    pub(crate) fn replace_text(&mut self, new: Buffer, remap: impl Fn(Pos) -> Pos + 'static) {
        self.rope = new.rope;
        self.crlf = new.crlf;
        self.touch();
        self.no_lines = new.no_lines;
        self.pos_edits.push(PosEdit::Remap(Box::new(remap)));
    }

    /// The lines of file contents `bytes`, decoded as `load` does (for
    /// `:read`). An empty file has no lines.
    pub fn file_lines(bytes: &[u8]) -> (Vec<String>, FileStats) {
        let d = decode(bytes);
        let lines = if d.no_lines {
            Vec::new()
        } else {
            d.text.split('\n').map(str::to_owned).collect()
        };
        (lines, d.stats)
    }

    /// File contents as written: every line gets a terminator ('fixeol').
    pub fn to_bytes(&self) -> Vec<u8> {
        if self.no_lines {
            return Vec::new();
        }
        let nl = if self.crlf { "\r\n" } else { "\n" };
        let mut out = String::with_capacity(self.rope.len_bytes() + self.line_count());
        for l in 0..self.line_count() {
            out.extend(self.line(l).chunks());
            out.push_str(nl);
        }
        out.into_bytes()
    }

    pub fn write_to(&self, path: &Path) -> io::Result<FileStats> {
        let bytes = self.to_bytes();
        fs::write(path, &bytes)?;
        Ok(FileStats {
            lines: if self.no_lines { 0 } else { self.line_count() },
            bytes: bytes.len(),
            noeol: false,
        })
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn last_line(&self) -> usize {
        self.line_count() - 1
    }

    /// Line `l` without its `\n`.
    pub fn line(&self, l: usize) -> RopeSlice<'_> {
        line_of(&self.rope, l)
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            rope: self.rope.clone(),
            no_lines: self.no_lines,
        }
    }

    /// Restores text from a snapshot (undo/redo). Marks the buffer changed.
    pub fn restore(&mut self, snap: &Snapshot) {
        self.rope = snap.rope.clone();
        self.no_lines = snap.no_lines;
        self.changedtick += 1;
    }

    /// The buffer has no lines at all (Vim's ML_EMPTY).
    pub fn has_no_lines(&self) -> bool {
        self.no_lines
    }

    /// Replaces the text of line `l`.
    pub fn set_line(&mut self, l: usize, text: &str) {
        let start = self.rope.line_to_char(l);
        let len = self.line_len(l);
        self.rope.remove(start..start + len);
        self.rope.insert(start, text);
        self.touch();
    }

    /// Deletes `n` whole lines starting at `l`. Deleting every line leaves
    /// one empty line and no "real" lines, as in Vim.
    pub fn delete_lines(&mut self, l: usize, n: usize) {
        let total = self.line_count();
        let n = n.min(total - l);
        if n == 0 {
            return;
        }
        let all = n == total;
        self.adjust_marks(l, n, 0);
        for t in self.tracked.iter_mut() {
            if *t == usize::MAX {
            } else if *t >= l + n {
                *t -= n;
            } else if *t >= l {
                *t = usize::MAX;
            }
        }
        if all {
            self.rope = Rope::new();
        } else if l + n < total {
            let start = self.rope.line_to_char(l);
            let end = self.rope.line_to_char(l + n);
            self.rope.remove(start..end);
        } else {
            // Deleting the last lines: remove the line break before them.
            let start = self.rope.line_to_char(l) - 1;
            let end = self.rope.len_chars();
            self.rope.remove(start..end);
        }
        self.touch();
        self.no_lines = all;
    }

    pub fn line_string(&self, l: usize) -> String {
        self.line(l).to_string()
    }

    pub fn line_len(&self, l: usize) -> usize {
        self.line(l).len_chars()
    }

    pub fn char_at(&self, l: usize, c: usize) -> Option<char> {
        let line = self.line(l);
        (c < line.len_chars()).then(|| line.char(c))
    }

    fn index(&self, l: usize, c: usize) -> usize {
        self.rope.line_to_char(l) + c
    }

    /// Records a change without changing text (Vim's changed_lines() on
    /// lines that ended up the same).
    pub fn mark_changed(&mut self) {
        self.modified = true;
        self.changedtick += 1;
    }

    fn touch(&mut self) {
        self.modified = true;
        self.no_lines = false;
        self.changedtick += 1;
    }

    /// Inserts `text` (which may contain `\n`) before (l, c). As in Vim, a
    /// line break does not move marks on line `l`, whatever their column.
    pub fn insert(&mut self, l: usize, c: usize, text: &str) {
        let breaks = text.matches('\n').count();
        if breaks > 0 {
            for t in self.tracked.iter_mut() {
                if *t != usize::MAX && *t > l {
                    *t += breaks;
                }
            }
            self.adjust_marks(l + 1, 0, breaks);
        }
        let i = self.index(l, c);
        self.rope.insert(i, text);
        self.touch();
    }

    /// Removes `n` chars starting at (l, c); a line break counts as one char.
    /// Removing a line break joins lines: marks on the joined line move to
    /// the join point, as Vim's `do_join()` does.
    pub fn remove(&mut self, l: usize, c: usize, n: usize) {
        let i = self.index(l, c);
        let end = (i + n).min(self.rope.len_chars());
        let breaks = self
            .rope
            .slice(i..end)
            .chars()
            .filter(|&ch| ch == '\n')
            .count();
        if breaks > 0 {
            let l2 = self.rope.char_to_line(end);
            let c2 = end - self.rope.line_to_char(l2);
            for from in l + 1..=l2 {
                self.move_marks_to_line(from, l, move |k| {
                    if from == l2 && k >= c2 { c + k - c2 } else { c }
                });
            }
            self.adjust_marks(l + 1, breaks, 0);
        }
        self.rope.remove(i..end);
        self.touch();
    }

    /// Inserts a new line so that it becomes line `at` (`at` may equal
    /// `line_count()` to append).
    pub fn insert_line(&mut self, at: usize, text: &str) {
        for t in self.tracked.iter_mut() {
            if *t != usize::MAX && *t >= at {
                *t += 1;
            }
        }
        self.adjust_marks(at, 0, 1);
        if at >= self.line_count() {
            let end = self.rope.len_chars();
            self.rope.insert(end, &format!("\n{text}"));
        } else {
            let i = self.rope.line_to_char(at);
            self.rope.insert(i, &format!("{text}\n"));
        }
        self.touch();
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn lines(&self) -> Vec<String> {
        (0..self.line_count())
            .map(|l| self.line_string(l))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_access() {
        let b = Buffer::from_text("ab\n\ncd");
        assert_eq!(b.line_count(), 3);
        assert_eq!(b.line_string(0), "ab");
        assert_eq!(b.line_len(1), 0);
        assert_eq!(b.char_at(2, 1), Some('d'));
        assert_eq!(b.char_at(2, 2), None);
    }

    #[test]
    fn edits() {
        let mut b = Buffer::from_text("ab\ncd");
        b.insert(0, 1, "X\n");
        assert_eq!(b.lines(), ["aX", "b", "cd"]);
        b.remove(1, 1, 1); // join "b" and "cd"
        assert_eq!(b.lines(), ["aX", "bcd"]);
        b.insert_line(2, "end");
        b.insert_line(0, "start");
        assert_eq!(b.lines(), ["start", "aX", "bcd", "end"]);
    }

    #[test]
    fn round_trips_files() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f.txt");
        for (content, crlf, noeol) in [
            ("a\nb\n", false, false),
            ("a\r\nb\r\n", true, false),
            ("\n", false, false),
            ("", false, false),
        ] {
            fs::write(&p, content).unwrap();
            let (b, stats) = Buffer::load(&p).unwrap();
            let stats = stats.unwrap();
            assert_eq!((b.crlf, stats.noeol), (crlf, noeol), "{content:?}");
            assert_eq!(b.to_bytes(), content.as_bytes(), "{content:?}");
        }
        // A missing final newline is added on write, as with 'fixeol'.
        fs::write(&p, "a\nb").unwrap();
        let (b, stats) = Buffer::load(&p).unwrap();
        assert_eq!(stats.unwrap().lines, 2);
        assert_eq!(b.to_bytes(), b"a\nb\n");
    }

    #[test]
    fn missing_file_is_new() {
        let dir = tempfile::tempdir().unwrap();
        let (b, stats) = Buffer::load(&dir.path().join("nope")).unwrap();
        assert!(stats.is_none());
        assert_eq!(b.line_count(), 1);
        assert!(b.to_bytes().is_empty());
    }
}

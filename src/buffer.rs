//! Text storage. A buffer is a list of lines held in a rope; lines are joined
//! by `\n` and the file's final newline is not stored (Vim's 'eol' model).
//! Positions are (line, char) pairs, both zero-based.

use ropey::{Rope, RopeSlice};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStats {
    pub lines: usize,
    pub bytes: usize,
    /// The file did not end with a newline.
    pub noeol: bool,
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
        }
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
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let buf = Self {
                    path: Some(path.to_owned()),
                    ..Self::new()
                };
                return Ok((buf, None));
            }
            Err(e) => return Err(e),
        };
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
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
        let buf = Self {
            rope: Rope::from_str(&text),
            path: Some(path.to_owned()),
            crlf,
            no_lines,
            ..Self::new()
        };
        Ok((buf, Some(stats)))
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
        let s = self.rope.line(l);
        if l + 1 < self.line_count() {
            s.slice(..s.len_chars() - 1)
        } else {
            s
        }
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

    fn touch(&mut self) {
        self.modified = true;
        self.no_lines = false;
        self.changedtick += 1;
    }

    /// Inserts `text` (which may contain `\n`) before (l, c).
    pub fn insert(&mut self, l: usize, c: usize, text: &str) {
        let i = self.index(l, c);
        self.rope.insert(i, text);
        self.touch();
    }

    /// Removes `n` chars starting at (l, c); a line break counts as one char.
    pub fn remove(&mut self, l: usize, c: usize, n: usize) {
        let i = self.index(l, c);
        let end = (i + n).min(self.rope.len_chars());
        self.rope.remove(i..end);
        self.touch();
    }

    /// Inserts a new line so that it becomes line `at` (`at` may equal
    /// `line_count()` to append).
    pub fn insert_line(&mut self, at: usize, text: &str) {
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

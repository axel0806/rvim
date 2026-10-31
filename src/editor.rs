//! The editor state machine: modes, counts and key dispatch. It has no
//! terminal dependency, so tests drive it with key strings.

use crate::buffer::{Buffer, FileStats};
use crate::key::{Key, parse_keys};
use crate::motion::{self, Pos};
use crate::text;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    CmdLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// How a counted insert (`3ifoo<Esc>`) repeats the typed text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertKind {
    /// Insert again at the cursor (i, a, I, A).
    Inline,
    /// Open a new line below the cursor first (o, O).
    OpenLine,
}

pub struct Editor {
    pub buf: Buffer,
    pub(crate) cursor: Pos,
    /// Desired virtual column for vertical motions; `usize::MAX` after `$`.
    pub(crate) curswant: usize,
    keep_curswant: bool,
    pub(crate) mode: Mode,
    count: Option<usize>,
    /// First key of a two-key command (`g`, `Z`).
    pending: Option<char>,
    insert_kind: InsertKind,
    insert_count: usize,
    insert_keys: Vec<Key>,
    pub(crate) cmdline: String,
    pub(crate) message: Option<Message>,
    pub(crate) quit: bool,
    bell: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Self::with_buffer(Buffer::new())
    }

    pub fn with_buffer(buf: Buffer) -> Self {
        Self {
            buf,
            cursor: Pos::default(),
            curswant: 0,
            keep_curswant: false,
            mode: Mode::Normal,
            count: None,
            pending: None,
            insert_kind: InsertKind::Inline,
            insert_count: 1,
            insert_keys: Vec::new(),
            cmdline: String::new(),
            message: None,
            quit: false,
            bell: false,
        }
    }

    pub fn from_text(text: &str) -> Self {
        Self::with_buffer(Buffer::from_text(text))
    }

    /// Opens `path` (a missing file starts a new buffer with that name).
    pub fn open(path: &Path) -> Self {
        let mut ed = Self::new();
        ed.edit_file(path);
        ed
    }

    /// Shows the startup notice (the "intro screen" message the Vim license
    /// asks for), unless a message such as file info is already shown.
    pub fn show_intro(&mut self) {
        if self.message.is_none() {
            self.info(crate::VERSION_MESSAGE);
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn cursor(&self) -> Pos {
        self.cursor
    }

    pub fn cmdline(&self) -> &str {
        &self.cmdline
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// True once if a command failed since the last call (Vim's beep).
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell)
    }

    /// Feeds keys in Vim notation, e.g. `"3wcwfoo<Esc>"`.
    pub fn feed(&mut self, keys: &str) {
        for k in parse_keys(keys) {
            self.handle_key(k);
        }
    }

    pub fn handle_key(&mut self, key: Key) {
        // Like Vim's w_set_curswant: nearly every command resets the desired
        // column, even if the cursor did not move. Vertical motions and
        // partial commands (counts, prefixes, command-line typing) opt out.
        self.keep_curswant = false;
        match self.mode {
            Mode::Normal => self.normal_key(key),
            Mode::Insert => self.insert_key(key),
            Mode::CmdLine => self.cmdline_key(key),
        }
        if self.mode == Mode::Normal {
            self.clamp_normal();
        }
        if !self.keep_curswant {
            self.curswant = self.cursor_vcol();
        }
    }

    // ---- helpers --------------------------------------------------------

    pub(crate) fn beep(&mut self) {
        self.bell = true;
    }

    pub(crate) fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
        self.beep();
    }

    pub(crate) fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    fn line_len(&self) -> usize {
        self.buf.line_len(self.cursor.line)
    }

    /// The cursor's virtual column as Vim shows it in the current mode.
    pub fn cursor_vcol(&self) -> usize {
        text::cursor_vcol(
            self.buf.line(self.cursor.line),
            self.cursor.col,
            self.mode == Mode::Insert,
        )
    }

    fn clamp_normal(&mut self) {
        self.cursor.line = self.cursor.line.min(self.buf.last_line());
        let len = self.line_len();
        if self.cursor.col >= len {
            self.cursor.col = len.saturating_sub(1);
        }
    }

    /// Puts the cursor at `curswant` on its line (Vim's coladvance).
    fn apply_curswant(&mut self) {
        self.cursor.col = text::col_for_vcol(
            self.buf.line(self.cursor.line),
            self.curswant,
            self.mode == Mode::Insert,
        );
        self.keep_curswant = true;
    }

    fn cursor_down(&mut self, n: usize) -> bool {
        if self.cursor.line >= self.buf.last_line() {
            return false;
        }
        self.cursor.line = (self.cursor.line + n).min(self.buf.last_line());
        true
    }

    fn cursor_up(&mut self, n: usize) -> bool {
        if self.cursor.line == 0 {
            return false;
        }
        self.cursor.line = self.cursor.line.saturating_sub(n);
        true
    }

    pub(crate) fn goto_line_first_non_blank(&mut self, line: usize) {
        self.keep_curswant = false;
        self.cursor.line = line.min(self.buf.last_line());
        self.cursor.col = motion::first_non_blank(&self.buf, self.cursor.line, true);
    }

    // ---- Normal mode ----------------------------------------------------

    fn normal_key(&mut self, key: Key) {
        if let Some(first) = self.pending.take() {
            let count = self.count.take();
            return self.normal_two_key(first, key, count);
        }
        if let Key::Char(c @ '0'..='9') = key
            && (c != '0' || self.count.is_some())
        {
            let d = c as usize - '0' as usize;
            self.count = Some(self.count.unwrap_or(0).saturating_mul(10).saturating_add(d));
            self.keep_curswant = true;
            return;
        }
        if key == Key::Delete && self.count.is_some() {
            // <Del> while typing a count removes its last digit.
            self.count = self.count.map(|c| c / 10).filter(|&c| c > 0);
            self.keep_curswant = true;
            return;
        }
        let count = self.count.take();
        let n = count.unwrap_or(1).max(1);
        let ok = match key {
            // Failed horizontal moves leave the desired column alone (below).
            Key::Char('h') | Key::Left => self.move_left(n, false),
            Key::Backspace => self.move_left(n, true),
            Key::Char('l') | Key::Right => self.move_right(n, false),
            Key::Char(' ') => self.move_right(n, true),
            Key::Char('j') | Key::Down | Key::Ctrl('n' | 'j') => self.vertical(n, true),
            Key::Char('k') | Key::Up | Key::Ctrl('p') => self.vertical(n, false),
            Key::Char('+') | Key::Enter => self.line_and_first_non_blank(n, true),
            Key::Char('-') => self.line_and_first_non_blank(n, false),
            Key::Char('0') | Key::Home => {
                self.cursor.col = 0;
                true
            }
            Key::Char('^') => {
                self.cursor.col = motion::first_non_blank(&self.buf, self.cursor.line, true);
                true
            }
            Key::Char('$') | Key::End => self.end_of_line(n),
            Key::Char(c @ ('w' | 'W')) => self.word_motion(motion::fwd_word, n, c == 'W'),
            Key::Char(c @ ('b' | 'B')) => self.word_motion(motion::bck_word, n, c == 'B'),
            Key::Char(c @ ('e' | 'E')) => self.word_motion(motion::end_word, n, c == 'E'),
            Key::Char('G') => {
                let line = count.map_or(self.buf.last_line(), |c| c.saturating_sub(1));
                self.goto_line_first_non_blank(line);
                true
            }
            Key::Char(c @ ('g' | 'Z')) => {
                self.pending = Some(c);
                self.count = count;
                self.keep_curswant = true;
                true
            }
            Key::Char('i') => {
                self.start_insert(n, InsertKind::Inline);
                true
            }
            Key::Char('a') => {
                if self.line_len() > 0 {
                    self.cursor.col += 1;
                }
                self.start_insert(n, InsertKind::Inline);
                true
            }
            Key::Char('I') => {
                self.cursor.col = motion::first_non_blank(&self.buf, self.cursor.line, false);
                self.start_insert(n, InsertKind::Inline);
                true
            }
            Key::Char('A') => {
                self.cursor.col = self.line_len();
                self.start_insert(n, InsertKind::Inline);
                true
            }
            Key::Char('o') => {
                self.open_line(true);
                self.start_insert(n, InsertKind::OpenLine);
                true
            }
            Key::Char('O') => {
                self.open_line(false);
                self.start_insert(n, InsertKind::OpenLine);
                true
            }
            Key::Char('x') | Key::Delete => self.delete_chars(n),
            Key::Char(':') => {
                self.keep_curswant = true;
                self.mode = Mode::CmdLine;
                self.cmdline.clear();
                self.message = None;
                true
            }
            Key::Ctrl('g') => {
                self.file_info();
                self.keep_curswant = true;
                true
            }
            Key::Ctrl('l') => {
                self.keep_curswant = true;
                true
            }
            // <Esc> only cancels a pending count; otherwise it beeps.
            Key::Esc => {
                self.keep_curswant = true;
                count.is_some()
            }
            _ => false,
        };
        if !ok {
            if matches!(
                key,
                Key::Char('h' | 'l' | ' ') | Key::Left | Key::Right | Key::Backspace
            ) {
                self.keep_curswant = true;
            }
            self.beep();
        }
    }

    fn normal_two_key(&mut self, first: char, key: Key, count: Option<usize>) {
        match (first, key) {
            ('g', Key::Char('g')) => {
                let line = count.unwrap_or(1).saturating_sub(1);
                self.goto_line_first_non_blank(line);
            }
            ('Z', Key::Char('Z')) => self.execute_ex("x"),
            ('Z', Key::Char('Q')) => self.execute_ex("q!"),
            _ => self.beep(),
        }
    }

    /// Only beeps if the cursor could not move at all, like Vim.
    fn move_left(&mut self, n: usize, wrap: bool) -> bool {
        for i in 0..n {
            if self.cursor.col > 0 {
                self.cursor.col -= 1;
            } else if wrap && self.cursor.line > 0 {
                self.cursor.line -= 1;
                self.cursor.col = self.line_len().saturating_sub(1);
            } else {
                return i > 0;
            }
        }
        true
    }

    fn move_right(&mut self, n: usize, wrap: bool) -> bool {
        for i in 0..n {
            if self.cursor.col + 1 < self.line_len() {
                self.cursor.col += 1;
            } else if wrap && self.cursor.line < self.buf.last_line() {
                self.cursor.line += 1;
                self.cursor.col = 0;
            } else {
                return i > 0;
            }
        }
        true
    }

    fn vertical(&mut self, n: usize, down: bool) -> bool {
        self.keep_curswant = true;
        let ok = if down {
            self.cursor_down(n)
        } else {
            self.cursor_up(n)
        };
        if ok {
            self.apply_curswant();
        }
        ok
    }

    fn line_and_first_non_blank(&mut self, n: usize, down: bool) -> bool {
        let ok = if down {
            self.cursor_down(n)
        } else {
            self.cursor_up(n)
        };
        if ok {
            self.cursor.col = motion::first_non_blank(&self.buf, self.cursor.line, true);
        }
        ok
    }

    fn end_of_line(&mut self, n: usize) -> bool {
        if n > 1 && !self.cursor_down(n - 1) {
            return false;
        }
        self.cursor.col = self.line_len().saturating_sub(1);
        self.curswant = usize::MAX;
        self.keep_curswant = true;
        true
    }

    fn word_motion(
        &mut self,
        f: fn(&Buffer, &mut Pos, usize, bool) -> bool,
        n: usize,
        big: bool,
    ) -> bool {
        let mut p = self.cursor;
        let ok = f(&self.buf, &mut p, n, big);
        self.cursor = p;
        ok
    }

    fn delete_chars(&mut self, n: usize) -> bool {
        let len = self.line_len();
        if len == 0 {
            return false;
        }
        let n = n.min(len - self.cursor.col);
        self.buf.remove(self.cursor.line, self.cursor.col, n);
        true
    }

    fn open_line(&mut self, below: bool) {
        let at = self.cursor.line + usize::from(below);
        self.buf.insert_line(at, "");
        self.cursor = Pos::new(at, 0);
    }

    fn file_info(&mut self) {
        let name = self.display_name();
        let lines = self.buf.line_count();
        let pct = (self.cursor.line + 1) * 100 / lines;
        let modified = if self.buf.modified { " [Modified]" } else { "" };
        self.info(format!(
            "\"{name}\"{modified} {lines} line{} --{pct}%--",
            if lines == 1 { "" } else { "s" }
        ));
    }

    pub(crate) fn display_name(&self) -> String {
        self.buf
            .path
            .as_ref()
            .map_or("[No Name]".to_string(), |p| p.display().to_string())
    }

    // ---- Insert mode ----------------------------------------------------

    fn start_insert(&mut self, count: usize, kind: InsertKind) {
        self.mode = Mode::Insert;
        self.insert_kind = kind;
        self.insert_count = count;
        self.insert_keys.clear();
        self.message = None;
    }

    fn insert_key(&mut self, key: Key) {
        match key {
            Key::Esc => return self.finish_insert(),
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End => {
                // Moving the cursor starts a new insert: the count is dropped.
                self.insert_count = 1;
                self.insert_keys.clear();
                self.insert_move(key);
                return;
            }
            _ => {}
        }
        if self.insert_edit(key) {
            self.insert_keys.push(key);
        }
    }

    /// Applies an editing key. Returns false for keys that do nothing.
    fn insert_edit(&mut self, key: Key) -> bool {
        let Pos { line, col } = self.cursor;
        match key {
            Key::Char(c) => {
                self.buf.insert(line, col, c.encode_utf8(&mut [0; 4]));
                self.cursor.col += 1;
            }
            Key::Tab => {
                self.buf.insert(line, col, "\t");
                self.cursor.col += 1;
            }
            Key::Enter | Key::Ctrl('j') => {
                self.buf.insert(line, col, "\n");
                self.cursor = Pos::new(line + 1, 0);
            }
            // 'backspace' is "indent,eol,start" (Vim's defaults.vim value).
            Key::Backspace => {
                if col > 0 {
                    self.buf.remove(line, col - 1, 1);
                    self.cursor.col -= 1;
                } else if line > 0 {
                    let prev_len = self.buf.line_len(line - 1);
                    self.buf.remove(line - 1, prev_len, 1);
                    self.cursor = Pos::new(line - 1, prev_len);
                } else {
                    self.beep();
                    return false;
                }
            }
            Key::Delete => {
                if col < self.line_len() || line < self.buf.last_line() {
                    self.buf.remove(line, col, 1);
                } else {
                    self.beep();
                    return false;
                }
            }
            _ => return false,
        }
        true
    }

    fn insert_move(&mut self, key: Key) {
        match key {
            Key::Left if self.cursor.col > 0 => self.cursor.col -= 1,
            Key::Right if self.cursor.col < self.line_len() => self.cursor.col += 1,
            Key::Up => {
                if !self.vertical(1, false) {
                    self.beep();
                }
            }
            Key::Down => {
                if !self.vertical(1, true) {
                    self.beep();
                }
            }
            Key::Home => self.cursor.col = 0,
            Key::End => self.cursor.col = self.line_len(),
            _ => self.beep(),
        }
    }

    fn finish_insert(&mut self) {
        let keys = std::mem::take(&mut self.insert_keys);
        for _ in 1..self.insert_count {
            if self.insert_kind == InsertKind::OpenLine {
                self.open_line(true);
            }
            for &k in &keys {
                self.insert_edit(k);
            }
        }
        self.insert_count = 1;
        self.mode = Mode::Normal;
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    // ---- Command-line mode ----------------------------------------------

    fn cmdline_key(&mut self, key: Key) {
        // Only commands that move the cursor reset the desired column.
        self.keep_curswant = true;
        match key {
            Key::Esc | Key::Ctrl('c') => {
                self.mode = Mode::Normal;
                self.cmdline.clear();
            }
            Key::Enter => {
                self.mode = Mode::Normal;
                let cmd = std::mem::take(&mut self.cmdline);
                self.execute_ex(&cmd);
            }
            Key::Backspace => {
                if self.cmdline.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            Key::Ctrl('u') => self.cmdline.clear(),
            Key::Char(c) => self.cmdline.push(c),
            Key::Tab => self.cmdline.push('\t'),
            _ => {}
        }
    }

    // ---- files ----------------------------------------------------------

    /// Replaces the buffer with `path`, reporting like Vim's `:e`.
    pub(crate) fn edit_file(&mut self, path: &Path) {
        match Buffer::load(path) {
            Ok((buf, stats)) => {
                self.buf = buf;
                self.cursor = Pos::default();
                self.curswant = 0;
                self.keep_curswant = false;
                let name = path.display();
                match stats {
                    None => self.info(format!("\"{name}\" [New]")),
                    Some(s) => self.info(format!("\"{name}\" {}", describe(s, self.buf.crlf))),
                }
            }
            Err(e) => self.error(format!("E484: Can't open file {}: {e}", path.display())),
        }
    }
}

/// `[noeol] [dos] 3L, 42B`
pub(crate) fn describe(s: FileStats, crlf: bool) -> String {
    let mut out = String::new();
    if s.noeol {
        out.push_str("[noeol] ");
    }
    if crlf {
        out.push_str("[dos] ");
    }
    out.push_str(&format!("{}L, {}B", s.lines, s.bytes));
    out
}

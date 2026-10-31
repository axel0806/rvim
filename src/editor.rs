//! The editor state machine: modes, Insert/Replace mode, the command line,
//! and the glue to undo, registers and `.`. Normal and Visual mode commands
//! are in normal.rs, operators in ops.rs. No terminal dependency: tests
//! drive it with key strings.

use crate::buffer::{Buffer, FileStats};
use crate::key::{Key, parse_keys};
use crate::motion::{self, Pos};
use crate::ops::OpArg;
use crate::register::Registers;
use crate::text;
use crate::undo::{Step, UndoTree};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualMode {
    Char,
    Line,
    Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Visual(VisualMode),
    CmdLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub error: bool,
}

/// How a counted insert (`3ifoo<Esc>`) repeats the typed text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertKind {
    /// Insert again at the cursor (i, a, I, A, R).
    Inline,
    /// Open a new line below the cursor first (o, O).
    OpenLine,
}

/// The last Visual area, for `gv` (Vim's `b_visual`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct LastVisual {
    pub start: Pos,
    pub end: Pos,
    pub mode: VisualMode,
    pub curswant: usize,
}

/// Size of a Visual operation, to repeat it with `.`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualRedo {
    pub mode: VisualMode,
    /// Width (or end column for multi-line charwise), `usize::MAX` for `$`.
    pub vcol: usize,
    pub line_count: usize,
    pub count: Option<usize>,
}

/// The last change, for `.` (Vim's redo buffer).
#[derive(Clone, Debug)]
pub(crate) struct Redo {
    pub reg: Option<char>,
    pub count: Option<usize>,
    /// Command keys, plus inserted text and `<Esc>` for inserts.
    pub keys: Vec<Key>,
    pub visual: Option<VisualRedo>,
}

/// Work left for when Insert mode ends (Visual block `c`, `I`, `A`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum PostInsert {
    None,
    BlockChange {
        oa: OpArg,
        pre_textlen: usize,
        textcol: usize,
    },
    BlockInsert {
        oa: OpArg,
        pre_textlen: i64,
        bd_textcol: usize,
        bd_textlen: usize,
    },
}

/// The line `U` restores (Vim's `b_u_line_*`).
#[derive(Clone, Debug)]
pub(crate) struct ULine {
    pub line: usize,
    pub text: String,
    pub col: usize,
}

/// What a Replace mode keystroke did, so `<BS>` can undo it.
#[derive(Clone, Copy, Debug)]
enum RepItem {
    Replaced(char),
    Appended,
    Newline,
}

/// Options changed with `:set`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub ignorecase: bool,
    pub smartcase: bool,
    pub wrapscan: bool,
}

pub struct Editor {
    pub buf: Buffer,
    pub(crate) cursor: Pos,
    /// Desired virtual column for vertical motions; `usize::MAX` after `$`.
    pub(crate) curswant: usize,
    pub(crate) keep_curswant: bool,
    pub(crate) mode: Mode,
    /// Keys of a Normal/Visual command still being typed.
    pub(crate) pending: Vec<Key>,
    /// The other end of the Visual area (Vim's `VIsual`).
    pub(crate) vstart: Pos,
    pub(crate) last_visual: Option<LastVisual>,
    ins_kind: InsertKind,
    ins_count: usize,
    ins_keys: Vec<Key>,
    ins_stack: Vec<RepItem>,
    pub(crate) ins_post: PostInsert,
    arrow_used: bool,
    pub(crate) undo: UndoTree,
    pub(crate) uline: Option<ULine>,
    pub(crate) regs: Registers,
    pub(crate) last_find: Option<(char, bool, bool)>,
    pub(crate) search: crate::search::SearchState,
    /// The last pattern used by `/` or `:s` (Vim's `RE_LAST`).
    pub(crate) last_pat: Option<String>,
    /// The last substitute string, for `~` in patterns.
    pub(crate) last_sub: String,
    /// Raw replacement of the last `:s`, for `~` and `:&`.
    pub(crate) last_sub_raw: String,
    pub(crate) sub_flags: crate::ex::SubFlags,
    pub(crate) opts: Options,
    pub(crate) global_busy: bool,
    pub(crate) cmd_literal: bool,
    pub(crate) global_need_beginline: bool,
    /// Text of the `/` or `?` command being executed.
    pub(crate) cmd_search: Option<String>,
    pub(crate) redo: Option<Redo>,
    pub(crate) redo_building: Option<Redo>,
    pub(crate) replaying_visual: Option<VisualRedo>,
    /// Size of the last Visual operation, for `1v` (Vim's resel_VIsual_*).
    pub(crate) resel: Option<VisualRedo>,
    depth: usize,
    /// The `'[` and `']` marks.
    pub(crate) op_start: Pos,
    pub(crate) op_end: Pos,
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
        let undo = UndoTree::new(buf.snapshot());
        Self {
            buf,
            cursor: Pos::default(),
            curswant: 0,
            keep_curswant: false,
            mode: Mode::Normal,
            pending: Vec::new(),
            vstart: Pos::default(),
            last_visual: None,
            ins_kind: InsertKind::Inline,
            ins_count: 1,
            ins_keys: Vec::new(),
            ins_stack: Vec::new(),
            ins_post: PostInsert::None,
            arrow_used: false,
            undo,
            uline: None,
            regs: Registers::new(),
            last_find: None,
            search: Default::default(),
            last_pat: None,
            last_sub: String::new(),
            cmd_search: None,
            last_sub_raw: String::new(),
            sub_flags: Default::default(),
            opts: Options {
                wrapscan: true,
                ..Default::default()
            },
            global_busy: false,
            cmd_literal: false,
            global_need_beginline: false,
            redo: None,
            redo_building: None,
            replaying_visual: None,
            resel: None,
            depth: 0,
            op_start: Pos::default(),
            op_end: Pos::default(),
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

    /// The other end of the Visual area, when Visual mode is active.
    pub fn visual_start(&self) -> Option<Pos> {
        matches!(self.mode, Mode::Visual(_)).then_some(self.vstart)
    }

    /// The `/` or `?` command being typed, with its prompt character.
    pub fn search_prompt(&self) -> Option<String> {
        let k = &self.pending;
        let i = (0..k.len()).find(|&i| match k[i] {
            Key::Char('/') => true,
            Key::Char('?') => !(i > 0 && k[i - 1] == Key::Char('g')),
            _ => false,
        })?;
        let Key::Char(lead) = k[i] else { return None };
        let mut s = String::new();
        for key in &k[i + 1..] {
            match key {
                Key::Char(c) => s.push(*c),
                Key::Tab => s.push('\t'),
                Key::Backspace => {
                    s.pop();
                }
                Key::Ctrl('u') => s.clear(),
                _ => {}
            }
        }
        Some(format!("{lead}{s}"))
    }

    pub fn cmdline(&self) -> &str {
        &self.cmdline
    }

    /// Keys of a command still being typed (Vim's 'showcmd').
    pub fn pending_keys(&self) -> &[Key] {
        &self.pending
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// The contents of register `name` as lines, and its type (`v`, `V` or
    /// `<C-v>` with width, as Vim's `getregtype()`).
    pub fn register(&self, name: char) -> Option<(String, Vec<String>)> {
        let r = self.regs.get(name)?;
        let kind = match r.kind {
            crate::register::RegType::Char => "v".to_string(),
            crate::register::RegType::Line => "V".to_string(),
            crate::register::RegType::Block(w) => format!("\x16{}", w + 1),
        };
        Some((kind, r.lines.clone()))
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
        let key = match key {
            Key::Alt(c) => {
                self.handle_key(Key::Esc);
                return self.handle_key(Key::Char(c));
            }
            Key::Insert
                if self.mode == Mode::Normal
                    && self
                        .pending
                        .iter()
                        .all(|k| matches!(k, Key::Char(c) if c.is_ascii_digit())) =>
            {
                Key::Char('i')
            }
            k => k,
        };
        // Like Vim's w_set_curswant: nearly every command resets the desired
        // column, even if the cursor did not move. Vertical motions and
        // partial commands (counts, prefixes, command-line typing) opt out.
        self.depth += 1;
        self.keep_curswant = false;
        match self.mode {
            Mode::Normal | Mode::Visual(_) => self.normal_key(key),
            Mode::Insert | Mode::Replace => self.insert_key(key),
            Mode::CmdLine => self.cmdline_key(key),
        }
        match self.mode {
            Mode::Normal => self.clamp_normal(),
            Mode::Visual(_) => {
                self.cursor.line = self.cursor.line.min(self.buf.last_line());
                self.cursor.col = self.cursor.col.min(self.buf.line_len(self.cursor.line));
            }
            _ => {}
        }
        if !self.keep_curswant {
            self.curswant = self.cursor_vcol();
        }
        self.depth -= 1;
        if self.depth == 0
            && matches!(self.mode, Mode::Normal | Mode::Visual(_) | Mode::CmdLine)
            && self.pending.is_empty()
        {
            self.sync_undo();
        }
    }

    /// Text delivered by the terminal as a bracketed paste. It is data, never
    /// commands: in Insert mode it is typed literally (one undo step, no
    /// mappings), in Normal mode it is inserted as if `i` had been pressed,
    /// in Visual mode it replaces the selection, and on the command line it
    /// is appended without newlines.
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let keys = text.chars().filter_map(|c| match c {
            '\n' => Some(Key::Enter),
            '\t' => Some(Key::Tab),
            c if c.is_control() => None,
            c => Some(Key::Char(c)),
        });
        match self.mode {
            Mode::Insert | Mode::Replace => keys.for_each(|k| self.handle_key(k)),
            Mode::CmdLine => self
                .cmdline
                .extend(text.chars().filter(|c| !c.is_control())),
            Mode::Normal | Mode::Visual(_) => {
                if !self.pending.is_empty() {
                    // Half-typed command: drop it so the paste cannot complete it.
                    self.pending.clear();
                }
                let start = if matches!(self.mode, Mode::Visual(_)) {
                    Key::Char('c')
                } else {
                    Key::Char('i')
                };
                self.handle_key(start);
                keys.for_each(|k| self.handle_key(k));
                self.handle_key(Key::Esc);
            }
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
            matches!(self.mode, Mode::Insert | Mode::Replace),
        )
    }

    fn clamp_normal(&mut self) {
        self.cursor.line = self.cursor.line.min(self.buf.last_line());
        let len = self.line_len();
        if self.cursor.col >= len {
            self.cursor.col = len.saturating_sub(1);
        }
    }

    pub(crate) fn goto_line_first_non_blank(&mut self, line: usize) {
        self.keep_curswant = false;
        self.cursor.line = line.min(self.buf.last_line());
        self.cursor.col = motion::first_non_blank(&self.buf, self.cursor.line, true);
    }

    pub(crate) fn file_info(&mut self) {
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

    // ---- undo -----------------------------------------------------------

    /// Vim's `u_save()`: lines `first..first + count` are about to change.
    /// A single saved line also becomes the line for `U` when `saveline`.
    pub(crate) fn u_save(&mut self, first: usize, count: usize, saveline: bool) {
        if saveline && count == 1 {
            self.u_saveline(first);
        }
        self.undo.save(
            first,
            count,
            self.buf.line_count(),
            self.cursor,
            self.buf.modified,
        );
    }

    pub(crate) fn u_save_cursor(&mut self) {
        self.u_save(self.cursor.line, 1, true);
    }

    fn u_saveline(&mut self, l: usize) {
        if self.uline.as_ref().is_some_and(|u| u.line == l) || l >= self.buf.line_count() {
            return;
        }
        self.uline = Some(ULine {
            line: l,
            text: self.buf.line_string(l),
            col: if self.cursor.line == l {
                self.cursor.col
            } else {
                0
            },
        });
    }

    /// Ends the current undoable change (Vim's `u_sync`).
    pub(crate) fn sync_undo(&mut self) {
        // An empty change (e.g. "x" on an empty line) is still a step, but
        // only real edits set 'modified' (Vim's changed()).
        if self.undo.is_open() {
            self.undo.sync(self.buf.snapshot());
        }
    }

    /// The buffer was written.
    pub(crate) fn mark_saved(&mut self) {
        self.sync_undo();
        self.undo.mark_saved();
        self.buf.modified = false;
    }

    /// `u` and `<C-r>`.
    pub(crate) fn undo_cmd(&mut self, count: usize, redo: bool) {
        self.sync_undo();
        let lines_before = self.buf.line_count() as i64;
        let mut done = 0;
        for _ in 0..count {
            let modified = self.buf.modified;
            let step = if redo {
                self.undo.redo(modified)
            } else {
                self.undo.undo(modified)
            };
            let Some(step) = step else { break };
            self.buf.restore(&step.snap);
            self.buf.modified = step.modified;
            self.cursor = self.undo_cursor(&step);
            done += 1;
        }
        if done == 0 {
            self.beep();
            self.info(if redo {
                "Already at newest change"
            } else {
                "Already at oldest change"
            });
            return;
        }
        let diff = self.buf.line_count() as i64 - lines_before;
        let what = match diff {
            1 => "1 more line".to_string(),
            d if d > 1 => format!("{d} more lines"),
            -1 => "1 line less".to_string(),
            d if d < -1 => format!("{} fewer lines", -d),
            _ => format!("{done} change{}", if done == 1 { "" } else { "s" }),
        };
        let when = if redo { "after" } else { "before" };
        self.info(format!(
            "{what}; {when} #{}",
            self.undo.seq() + usize::from(!redo)
        ));
    }

    /// Where the cursor goes after an undo or redo step (Vim's
    /// `u_undoredo()`).
    fn undo_cursor(&self, step: &Step) -> Pos {
        let from = &step.from;
        let to = &step.snap;
        let top = step.top;
        let oldsize = from.line_count().saturating_sub(top + step.below);
        let newsize = to.line_count().saturating_sub(top + step.below);
        let uh = step.uh_cursor;
        let mut cursor = self.cursor;
        let lnum = uh.line + 1;
        if lnum >= top && lnum <= top + newsize + 1 {
            cursor = uh;
        } else {
            let mut i = 0;
            while i < newsize && i < oldsize {
                if to.line(top + i) != from.line(top + i) {
                    break;
                }
                i += 1;
            }
            if i == newsize {
                cursor.line = top;
            } else if i < newsize {
                cursor.line = top + i;
            }
        }
        if uh.line + 1 == cursor.line && cursor.line > 0 {
            cursor.line -= 1;
        }
        if cursor.line < self.buf.line_count() {
            if uh.line == cursor.line {
                cursor.col = uh.col;
            } else {
                cursor.col = motion::first_non_blank(&self.buf, cursor.line, true);
            }
        } else {
            cursor.line = self.buf.last_line();
            cursor.col = 0;
        }
        cursor
    }

    /// `U`: restores the last changed line (Vim's `u_undoline()`).
    pub(crate) fn undo_line(&mut self) {
        let Some(u) = self
            .uline
            .clone()
            .filter(|u| u.line < self.buf.line_count())
        else {
            self.beep();
            return;
        };
        self.u_save(u.line, 1, false);
        let old = self.buf.line_string(u.line);
        self.buf.set_line(u.line, &u.text);
        let col = if self.cursor.line == u.line {
            self.cursor.col
        } else {
            u.col
        };
        self.uline = Some(ULine {
            line: u.line,
            text: old,
            col,
        });
        self.cursor = Pos::new(u.line, u.col);
        let len = self.line_len();
        if self.cursor.col >= len {
            self.cursor.col = len.saturating_sub(1);
        }
    }

    // ---- Insert and Replace mode ------------------------------------------

    pub(crate) fn start_insert(&mut self, count: usize, kind: InsertKind, replace: bool) {
        self.mode = if replace { Mode::Replace } else { Mode::Insert };
        self.ins_kind = kind;
        self.ins_count = count;
        self.ins_keys.clear();
        self.ins_stack.clear();
        self.ins_post = PostInsert::None;
        self.arrow_used = false;
        self.message = None;
    }

    fn insert_key(&mut self, key: Key) {
        match key {
            Key::Esc | Key::Ctrl('c') => return self.finish_insert(),
            // Unmapped function keys insert their name, as in Vim.
            Key::F(n) => {
                // In Replace mode Vim replaces one character and inserts the
                // rest of the name.
                let mode = self.mode;
                for (i, c) in format!("<F{n}>").chars().enumerate() {
                    if i == 1 {
                        self.mode = Mode::Insert;
                    }
                    self.insert_key(Key::Char(c));
                }
                self.mode = mode;
                return;
            }
            Key::BackTab => return self.insert_key(Key::Tab),
            Key::Insert => {
                self.mode = if self.mode == Mode::Replace {
                    Mode::Insert
                } else {
                    Mode::Replace
                };
                return;
            }
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Home | Key::End => {
                // Moving the cursor starts a new insert: the count is dropped
                // and a new undo step begins (Vim's start_arrow).
                self.ins_count = 1;
                self.ins_keys.clear();
                self.ins_stack.clear();
                self.arrow_used = true;
                self.sync_undo();
                self.insert_move(key);
                return;
            }
            _ => {}
        }
        if self.arrow_used
            && matches!(
                key,
                Key::Char(_) | Key::Tab | Key::Enter | Key::Backspace | Key::Delete
            )
        {
            // Vim's stop_arrow: "." now repeats only what is typed from here.
            self.arrow_used = false;
            if self.replaying_visual.is_none() {
                let c = if self.mode == Mode::Replace { 'R' } else { 'i' };
                self.redo_building = Some(Redo {
                    reg: None,
                    count: Some(1),
                    keys: vec![Key::Char(c)],
                    visual: None,
                });
            }
        }
        if self.insert_edit(key) {
            self.ins_keys.push(key);
            if let Some(r) = &mut self.redo_building {
                r.keys.push(key);
            }
        }
    }

    /// Applies an editing key. Returns false for keys that do nothing.
    fn insert_edit(&mut self, key: Key) -> bool {
        let Pos { line, col } = self.cursor;
        let replace = self.mode == Mode::Replace;
        match key {
            Key::Char(_) | Key::Tab => {
                let c = if let Key::Char(c) = key { c } else { '\t' };
                self.u_save_cursor();
                if replace && col < self.line_len() {
                    let old = self.buf.char_at(line, col).unwrap_or(' ');
                    self.buf.remove(line, col, 1);
                    self.buf.insert(line, col, c.encode_utf8(&mut [0; 4]));
                    self.ins_stack.push(RepItem::Replaced(old));
                } else {
                    self.buf.insert(line, col, c.encode_utf8(&mut [0; 4]));
                    self.ins_stack.push(RepItem::Appended);
                }
                self.cursor.col += 1;
            }
            Key::Enter | Key::Ctrl('j') => {
                self.u_save_cursor();
                self.uline = None;
                self.buf.insert(line, col, "\n");
                self.cursor = Pos::new(line + 1, 0);
                self.ins_stack.push(RepItem::Newline);
            }
            Key::Backspace if replace => match self.ins_stack.pop() {
                Some(RepItem::Replaced(c)) => {
                    self.u_save_cursor();
                    self.cursor.col -= 1;
                    self.buf.remove(line, col - 1, 1);
                    self.buf.insert(line, col - 1, c.encode_utf8(&mut [0; 4]));
                }
                Some(RepItem::Appended) => {
                    self.u_save_cursor();
                    self.cursor.col -= 1;
                    self.buf.remove(line, col - 1, 1);
                }
                Some(RepItem::Newline) => {
                    let prev_len = self.buf.line_len(line - 1);
                    self.u_save(line - 1, 2, true);
                    self.buf.remove(line - 1, prev_len, 1);
                    self.cursor = Pos::new(line - 1, prev_len);
                }
                None => {
                    if col > 0 {
                        self.cursor.col -= 1;
                    } else {
                        self.beep();
                    }
                    return false;
                }
            },
            // 'backspace' is "indent,eol,start" (Vim's defaults).
            Key::Backspace => {
                if col > 0 {
                    self.u_save_cursor();
                    self.buf.remove(line, col - 1, 1);
                    self.cursor.col -= 1;
                } else if line > 0 {
                    let prev_len = self.buf.line_len(line - 1);
                    self.u_save(line - 1, 2, true);
                    self.buf.remove(line - 1, prev_len, 1);
                    self.cursor = Pos::new(line - 1, prev_len);
                } else {
                    self.beep();
                    return false;
                }
            }
            Key::Delete => {
                if col < self.line_len() {
                    self.u_save_cursor();
                    self.buf.remove(line, col, 1);
                } else if line < self.buf.last_line() {
                    self.u_save(line, 2, true);
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
            Key::Up | Key::Down => {
                self.keep_curswant = true;
                let ok = if key == Key::Down {
                    self.cursor_down_n(1)
                } else {
                    self.cursor_up_n(1)
                };
                if ok {
                    self.coladvance(self.curswant);
                } else {
                    self.beep();
                }
            }
            Key::Home => self.cursor.col = 0,
            Key::End => self.cursor.col = self.line_len(),
            _ => self.beep(),
        }
    }

    fn finish_insert(&mut self) {
        let keys = std::mem::take(&mut self.ins_keys);
        for _ in 1..self.ins_count {
            if self.ins_kind == InsertKind::OpenLine {
                let at = self.cursor.line + 1;
                self.u_save(at, 0, false);
                self.uline = None;
                self.buf.insert_line(at, "");
                self.cursor = Pos::new(at, 0);
            }
            for &k in &keys {
                self.insert_edit(k);
            }
        }
        self.ins_count = 1;
        self.mode = Mode::Normal;
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
        match std::mem::replace(&mut self.ins_post, PostInsert::None) {
            PostInsert::None => {}
            PostInsert::BlockChange {
                oa,
                pre_textlen,
                textcol,
            } => {
                self.finish_block_change(oa, pre_textlen, textcol);
            }
            PostInsert::BlockInsert {
                oa,
                pre_textlen,
                bd_textcol,
                bd_textlen,
            } => self.finish_block_insert(oa, pre_textlen, bd_textcol, bd_textlen),
        }
        if let Some(mut r) = self.redo_building.take() {
            r.keys.push(Key::Esc);
            self.redo = Some(r);
        }
    }

    // ---- Command-line mode ----------------------------------------------

    fn cmdline_key(&mut self, key: Key) {
        // Only commands that move the cursor reset the desired column.
        self.keep_curswant = true;
        if std::mem::take(&mut self.cmd_literal) {
            match key {
                Key::Char(c) => self.cmdline.push(c),
                Key::Esc => self.cmdline.push('\x1b'),
                Key::Enter => self.cmdline.push('\r'),
                Key::Tab => self.cmdline.push('\t'),
                Key::Backspace => self.cmdline.push('\x08'),
                Key::Ctrl(c) => self.cmdline.push(((c as u8) & 0x1f) as char),
                _ => {}
            }
            return;
        }
        match key {
            Key::Ctrl('v') | Key::Ctrl('q') => self.cmd_literal = true,
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
                self.undo = UndoTree::new(self.buf.snapshot());
                self.uline = None;
                self.last_visual = None;
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

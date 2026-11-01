// SPDX-License-Identifier: Vim
// Derived in part from Vim (https://www.vim.org); see LICENSE-VIM.
// This is a modified version, not the official Vim.
//! The editor state machine: modes, Insert/Replace mode, the command line,
//! and the glue to undo, registers and `.`. Normal and Visual mode commands
//! are in normal.rs, operators in ops.rs. No terminal dependency: tests
//! drive it with key strings.

use crate::buffer::{Buffer, FileStats};
use crate::document::{Document, ULine};
use crate::jumplist::JumpList;
use crate::key::{Key, parse_keys};
use crate::motion::{self, Pos};
use crate::ops::OpArg;
use crate::options::Options;
use crate::register::Registers;
use crate::text;
use crate::undo::Step;
use crate::window::Window;
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
    /// Waiting for the answer to a `c` flag prompt of `:s` (`y n a q l`).
    Confirm,
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

/// What a Replace mode keystroke did, so `<BS>` can undo it.
#[derive(Clone, Copy, Debug)]
enum RepItem {
    Replaced(char),
    Appended,
    Newline,
}

pub struct Editor {
    /// The document shown in the one window (PLAN.md, "Document, Window, Editor").
    pub(crate) doc: Document,
    /// The cursor, desired column and Visual start, in the one window.
    pub(crate) win: Window,
    pub(crate) mode: Mode,
    /// Keys of a Normal/Visual command still being typed.
    pub(crate) pending: Vec<Key>,
    /// The jumplist and the context mark (Vim's window state, see jumplist.rs).
    pub(crate) jumps: JumpList,
    ins_kind: InsertKind,
    ins_count: usize,
    ins_keys: Vec<Key>,
    ins_stack: Vec<RepItem>,
    pub(crate) ins_post: PostInsert,
    arrow_used: bool,
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
    /// A `:s` waiting for a `c` flag answer (mode `Confirm`).
    pub(crate) sub: Option<Box<crate::ex::SubRun>>,
    /// Ex command lines and `:g` steps still to run, last on top.
    pub(crate) ex_tasks: Vec<crate::ex::ExTask>,
    pub(crate) opts: Options,
    /// The current line holds only an automatic indent (Vim's `did_ai`).
    pub(crate) did_ai: bool,
    /// A space was typed in this insert (Vim's `inserted_space`): with
    /// 'softtabstop', BS then deletes one typed space at a time.
    pub(crate) inserted_space: bool,
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
    /// First buffer line shown in the window (Vim's `w_topline`).
    pub(crate) top: usize,
    /// Screen size in cells; the last row is the command line.
    pub(crate) screen_cols: usize,
    pub(crate) screen_rows: usize,
    /// The 'scroll' option for CTRL-D and CTRL-U; 0 means half the window.
    pub(crate) scroll_opt: usize,
    depth: usize,
    /// The `'[` and `']` marks.
    pub(crate) op_start: Pos,
    pub(crate) op_end: Pos,
    pub(crate) cmdline: String,
    pub(crate) cmdpos: usize,
    /// Register being recorded into (`q{reg}`), and the keys typed so far.
    pub(crate) recording: Option<char>,
    pub(crate) rec_keys: Vec<Key>,
    pub(crate) last_macro: Option<char>,
    /// Number of errors so far; a macro stops when it grows.
    pub(crate) beeps: usize,
    /// Nesting level of register execution (keys are not recorded inside).
    pub(crate) replaying: usize,
    pub(crate) cmd_kind: crate::cmdline::PromptKind,
    /// The mode the command line returns to (Visual for a search from Visual).
    pub(crate) cmd_return: Mode,
    /// Keys typed before a search prompt (count, register, operator).
    pub(crate) cmd_prefix: Vec<Key>,
    pub(crate) cmd_hist: Vec<String>,
    pub(crate) search_hist: Vec<String>,
    pub(crate) hist_idx: Option<usize>,
    pub(crate) hist_prefix: String,
    pub(crate) cmd_paste: Option<crate::cmdline::PasteState>,
    /// The start menu, while it is shown (see startmenu.rs).
    pub(crate) start_menu: Option<crate::startmenu::StartMenu>,
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
            doc: Document::new(buf),
            win: Window::default(),
            mode: Mode::Normal,
            pending: Vec::new(),
            jumps: JumpList::default(),
            ins_kind: InsertKind::Inline,
            ins_count: 1,
            ins_keys: Vec::new(),
            ins_stack: Vec::new(),
            ins_post: PostInsert::None,
            arrow_used: false,
            regs: Registers::new(),
            last_find: None,
            search: Default::default(),
            last_pat: None,
            last_sub: String::new(),
            cmd_search: None,
            last_sub_raw: String::new(),
            sub_flags: Default::default(),
            sub: None,
            ex_tasks: Vec::new(),
            opts: Options::default(),
            did_ai: false,
            inserted_space: false,
            global_busy: false,
            cmd_literal: false,
            global_need_beginline: false,
            redo: None,
            redo_building: None,
            replaying_visual: None,
            resel: None,
            top: 0,
            // Vim's default screen when it runs without a terminal.
            screen_cols: 80,
            screen_rows: 24,
            scroll_opt: 0,
            depth: 0,
            op_start: Pos::default(),
            op_end: Pos::default(),
            cmdline: String::new(),
            cmdpos: 0,
            recording: None,
            rec_keys: Vec::new(),
            last_macro: None,
            beeps: 0,
            replaying: 0,
            cmd_kind: crate::cmdline::PromptKind::Ex,
            cmd_return: Mode::Normal,
            cmd_prefix: Vec::new(),
            cmd_hist: Vec::new(),
            search_hist: Vec::new(),
            hist_idx: None,
            hist_prefix: String::new(),
            cmd_paste: None,
            start_menu: None,
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
        self.win.cursor
    }

    /// The text of the document shown in the window.
    pub fn buf(&self) -> &Buffer {
        &self.doc.buf
    }

    /// The other end of the Visual area, when the area is shown.
    pub fn visual_start(&self) -> Option<Pos> {
        self.shown_visual().map(|_| self.win.vstart)
    }

    /// The Visual mode whose area is drawn. That includes a `/` or `?` prompt
    /// started from Visual mode, which keeps the area on screen.
    pub fn shown_visual(&self) -> Option<VisualMode> {
        match (self.mode, self.cmd_return) {
            (Mode::Visual(m), _) | (Mode::CmdLine, Mode::Visual(m)) => Some(m),
            _ => None,
        }
    }

    /// The text of the command line (after its prompt character).
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

    /// The single entry point for everything that happens to the editor.
    pub fn handle_event(&mut self, ev: crate::event::Event) {
        use crate::event::Event;
        match ev {
            Event::Key(k) => self.handle_key(k),
            Event::Paste(text) => self.paste(&text),
            Event::Resize(cols, rows) => self.set_screen_size(cols as usize, rows as usize),
            Event::FocusGained | Event::FocusLost | Event::Timer | Event::Signal(_) => {}
        }
    }

    /// When the front end should send [`Event::Timer`](crate::event::Event::Timer)
    /// if nothing else happens first. `None` means no timer is needed (there
    /// are no mappings with `timeoutlen` yet).
    pub fn next_deadline(&self) -> Option<std::time::Instant> {
        None
    }

    pub fn handle_key(&mut self, key: Key) {
        // The start menu takes its own keys; any other key closes it and is
        // handled below as usual.
        if self.start_menu_key(key) {
            return;
        }
        // Recording: the `q` that ends it is not part of the macro; keys
        // replayed from a register are not recorded again.
        if self.depth == 0 && self.replaying == 0 && self.recording.is_some() {
            if key == Key::Char('q') && self.mode == Mode::Normal && self.pending.is_empty() {
                return self.stop_recording();
            }
            self.rec_keys.push(key);
        }
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
        self.win.keep_curswant = false;
        match self.mode {
            Mode::Normal | Mode::Visual(_) => self.normal_key(key),
            Mode::Insert | Mode::Replace => self.insert_key(key),
            Mode::CmdLine => self.cmdline_key(key),
            Mode::Confirm => self.confirm_key(key),
        }
        match self.mode {
            Mode::Normal => self.clamp_normal(),
            Mode::Visual(_) => {
                self.win.cursor.line = self.win.cursor.line.min(self.doc.buf.last_line());
                self.win.cursor.col = self
                    .win
                    .cursor
                    .col
                    .min(self.doc.buf.line_len(self.win.cursor.line));
            }
            _ => {}
        }
        if !self.win.keep_curswant {
            self.win.curswant = self.cursor_vcol();
        }
        self.update_topline();
        self.sync_jumps();
        // Vim's checkpcmark() at the end of each command. Insert and command
        // line mode are part of the command that started them, so it runs
        // when they end.
        if matches!(self.mode, Mode::Normal | Mode::Visual(_)) && self.pending.is_empty() {
            self.jumps.check_pcmark(self.win.cursor);
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
        self.close_start_menu();
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let keys = text.chars().filter_map(|c| match c {
            '\n' => Some(Key::Enter),
            '\t' => Some(Key::Tab),
            c if c.is_control() => None,
            c => Some(Key::Char(c)),
        });
        match self.mode {
            Mode::Insert | Mode::Replace => keys.for_each(|k| self.handle_key(k)),
            Mode::CmdLine => {
                let t: String = text.chars().filter(|c| !c.is_control()).collect();
                self.cmd_insert(&t);
            }
            // Pasted text is data, not an answer to a `:s` prompt.
            Mode::Confirm => {}
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
        self.beeps += 1;
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
        self.doc.buf.line_len(self.win.cursor.line)
    }

    /// The cursor's virtual column as Vim shows it in the current mode.
    pub fn cursor_vcol(&self) -> usize {
        text::cursor_vcol(
            self.doc.buf.line(self.win.cursor.line),
            self.win.cursor.col,
            matches!(self.mode, Mode::Insert | Mode::Replace),
            self.opts.tabstop(),
        )
    }

    fn clamp_normal(&mut self) {
        self.win.cursor.line = self.win.cursor.line.min(self.doc.buf.last_line());
        let len = self.line_len();
        if self.win.cursor.col >= len {
            self.win.cursor.col = len.saturating_sub(1);
        }
    }

    pub(crate) fn goto_line_first_non_blank(&mut self, line: usize) {
        self.win.keep_curswant = false;
        self.win.cursor.line = line.min(self.doc.buf.last_line());
        self.win.cursor.col = motion::first_non_blank(&self.doc.buf, self.win.cursor.line, true);
    }

    pub(crate) fn file_info(&mut self) {
        let name = self.display_name();
        let lines = self.doc.buf.line_count();
        let pct = (self.win.cursor.line + 1) * 100 / lines;
        let modified = if self.doc.buf.modified {
            " [Modified]"
        } else {
            ""
        };
        self.info(format!(
            "\"{name}\"{modified} {lines} line{} --{pct}%--",
            if lines == 1 { "" } else { "s" }
        ));
    }

    pub(crate) fn display_name(&self) -> String {
        self.doc
            .buf
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
        self.doc.undo.save(
            first,
            count,
            self.doc.buf.line_count(),
            self.win.cursor,
            self.doc.buf.modified,
            self.doc.buf.marks,
        );
    }

    pub(crate) fn u_save_cursor(&mut self) {
        self.u_save(self.win.cursor.line, 1, true);
    }

    fn u_saveline(&mut self, l: usize) {
        if self.doc.uline.as_ref().is_some_and(|u| u.line == l) || l >= self.doc.buf.line_count() {
            return;
        }
        self.doc.uline = Some(ULine {
            line: l,
            text: self.doc.buf.line_string(l),
            col: if self.win.cursor.line == l {
                self.win.cursor.col
            } else {
                0
            },
        });
    }

    /// Ends the current undoable change (Vim's `u_sync`).
    pub(crate) fn sync_undo(&mut self) {
        // An empty change (e.g. "x" on an empty line) is still a step, but
        // only real edits set 'modified' (Vim's changed()).
        if self.doc.undo.is_open() {
            self.doc.undo.sync(self.doc.buf.snapshot());
        }
    }

    /// The buffer was written.
    pub(crate) fn mark_saved(&mut self) {
        self.sync_undo();
        self.doc.undo.mark_saved();
        self.doc.buf.modified = false;
    }

    /// `u` and `<C-r>`.
    pub(crate) fn undo_cmd(&mut self, count: usize, redo: bool) {
        self.sync_undo();
        let lines_before = self.doc.buf.line_count() as i64;
        let mut done = 0;
        for _ in 0..count {
            let modified = self.doc.buf.modified;
            let marks = self.doc.buf.marks;
            let step = if redo {
                self.doc.undo.redo(modified, marks)
            } else {
                self.doc.undo.undo(modified, marks)
            };
            let Some(step) = step else { break };
            // Undo and redo set the context mark to the cursor before the
            // step, as Vim does (the jumplist changes with it).
            self.set_pcmark();
            // Like Vim's u_undoredo(): when the number of lines changes,
            // marks on the replaced lines are deleted and the ones below
            // move. Then the marks saved with the change come back.
            let oldsize = step.from.line_count().saturating_sub(step.top + step.below);
            let newsize = step.snap.line_count().saturating_sub(step.top + step.below);
            if oldsize != newsize {
                self.doc.buf.adjust_marks(step.top, oldsize, newsize);
            }
            for (slot, saved) in self.doc.buf.marks.iter_mut().zip(step.marks) {
                if saved.is_some() {
                    *slot = saved;
                }
            }
            self.doc.buf.restore(&step.snap);
            self.doc.buf.modified = step.modified;
            self.win.cursor = self.undo_cursor(&step);
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
        let diff = self.doc.buf.line_count() as i64 - lines_before;
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
            self.doc.undo.seq() + usize::from(!redo)
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
        let mut cursor = self.win.cursor;
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
        if cursor.line < self.doc.buf.line_count() {
            if uh.line == cursor.line {
                cursor.col = uh.col;
            } else {
                cursor.col = motion::first_non_blank(&self.doc.buf, cursor.line, true);
            }
        } else {
            cursor.line = self.doc.buf.last_line();
            cursor.col = 0;
        }
        cursor
    }

    /// `U`: restores the last changed line (Vim's `u_undoline()`).
    pub(crate) fn undo_line(&mut self) {
        let Some(u) = self
            .doc
            .uline
            .clone()
            .filter(|u| u.line < self.doc.buf.line_count())
        else {
            self.beep();
            return;
        };
        self.u_save(u.line, 1, false);
        let old = self.doc.buf.line_string(u.line);
        self.doc.buf.set_line(u.line, &u.text);
        let col = if self.win.cursor.line == u.line {
            self.win.cursor.col
        } else {
            u.col
        };
        self.doc.uline = Some(ULine {
            line: u.line,
            text: old,
            col,
        });
        self.win.cursor = Pos::new(u.line, u.col);
        let len = self.line_len();
        if self.win.cursor.col >= len {
            self.win.cursor.col = len.saturating_sub(1);
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
        self.inserted_space = false;
        self.message = None;
    }

    /// Gives line `new` the indent of line `from` when 'autoindent' is on
    /// (the start of Vim's `open_line()`), leaving the cursor after it.
    pub(crate) fn auto_indent(&mut self, new: usize, from: usize) {
        if !self.opts.autoindent() {
            return;
        }
        let ind = Self::get_indent(&self.line_vec(from), self.opts.tabstop());
        if ind > 0 {
            self.set_indent(new, ind);
            let w = self
                .line_vec(new)
                .iter()
                .take_while(|&&c| c == ' ' || c == '\t')
                .count();
            self.win.cursor = Pos::new(new, w);
            self.did_ai = true;
        }
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
        match key {
            Key::Char(' ') => self.inserted_space = true,
            Key::Char(_) | Key::Tab => self.inserted_space = false,
            _ => {}
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
        let Pos { line, col } = self.win.cursor;
        let replace = self.mode == Mode::Replace;
        match key {
            Key::Char(_) | Key::Tab => {
                self.did_ai = false;
                if key == Key::Tab && self.ins_tab() {
                    return true;
                }
                let c = if let Key::Char(c) = key { c } else { '\t' };
                self.u_save_cursor();
                if replace && col < self.line_len() {
                    let old = self.doc.buf.char_at(line, col).unwrap_or(' ');
                    self.doc.buf.remove(line, col, 1);
                    self.doc.buf.insert(line, col, c.encode_utf8(&mut [0; 4]));
                    self.ins_stack.push(RepItem::Replaced(old));
                } else {
                    self.doc.buf.insert(line, col, c.encode_utf8(&mut [0; 4]));
                    self.ins_stack.push(RepItem::Appended);
                }
                self.win.cursor.col += 1;
            }
            Key::Enter | Key::Ctrl('j') => {
                self.u_save_cursor();
                self.doc.uline = None;
                let ai = self.opts.autoindent();
                let ind = if ai {
                    // Only the indent before the cursor is copied.
                    Self::get_indent(&self.line_vec(line)[..col], self.opts.tabstop())
                } else {
                    0
                };
                let was_ai = std::mem::take(&mut self.did_ai);
                self.doc.buf.insert(line, col, "\n");
                if ai {
                    // Text after the cursor loses its leading white space,
                    // and an indent that was never used is removed.
                    let skip = self
                        .line_vec(line + 1)
                        .iter()
                        .take_while(|&&c| c == ' ' || c == '\t')
                        .count();
                    if skip > 0 {
                        self.doc.buf.remove(line + 1, 0, skip);
                    }
                    if was_ai {
                        let mut len = self.doc.buf.line_len(line);
                        while len > 0
                            && matches!(self.doc.buf.char_at(line, len - 1), Some(' ' | '\t'))
                        {
                            self.doc.buf.remove(line, len - 1, 1);
                            len -= 1;
                        }
                    }
                    if ind > 0 {
                        self.set_indent(line + 1, ind);
                        self.did_ai = true;
                    }
                }
                let w = if ai {
                    self.line_vec(line + 1)
                        .iter()
                        .take_while(|&&c| c == ' ' || c == '\t')
                        .count()
                } else {
                    0
                };
                self.win.cursor = Pos::new(line + 1, w);
                self.ins_stack.push(RepItem::Newline);
            }
            Key::Backspace if replace => match self.ins_stack.pop() {
                Some(RepItem::Replaced(c)) => {
                    self.u_save_cursor();
                    self.win.cursor.col -= 1;
                    self.doc.buf.remove(line, col - 1, 1);
                    self.doc
                        .buf
                        .insert(line, col - 1, c.encode_utf8(&mut [0; 4]));
                }
                Some(RepItem::Appended) => {
                    self.u_save_cursor();
                    self.win.cursor.col -= 1;
                    self.doc.buf.remove(line, col - 1, 1);
                }
                Some(RepItem::Newline) => {
                    let prev_len = self.doc.buf.line_len(line - 1);
                    self.u_save(line - 1, 2, true);
                    self.doc.buf.remove(line - 1, prev_len, 1);
                    self.win.cursor = Pos::new(line - 1, prev_len);
                }
                None => {
                    if col > 0 {
                        self.win.cursor.col -= 1;
                    } else {
                        self.beep();
                    }
                    return false;
                }
            },
            Key::Backspace if self.bs_uses_sts() => self.ins_bs_sts(),
            // 'backspace' is "indent,eol,start" (Vim's defaults).
            Key::Backspace => {
                if col > 0 {
                    self.u_save_cursor();
                    self.doc.buf.remove(line, col - 1, 1);
                    self.win.cursor.col -= 1;
                } else if line > 0 {
                    let prev_len = self.doc.buf.line_len(line - 1);
                    self.u_save(line - 1, 2, true);
                    self.doc.buf.remove(line - 1, prev_len, 1);
                    self.win.cursor = Pos::new(line - 1, prev_len);
                } else {
                    self.beep();
                    return false;
                }
            }
            Key::Delete => {
                if col < self.line_len() {
                    self.u_save_cursor();
                    self.doc.buf.remove(line, col, 1);
                } else if line < self.doc.buf.last_line() {
                    self.u_save(line, 2, true);
                    self.doc.buf.remove(line, col, 1);
                } else {
                    self.beep();
                    return false;
                }
            }
            _ => return false,
        }
        true
    }

    /// True when only white space precedes the cursor (Vim's `inindent(0)`).
    fn in_indent(&self) -> bool {
        self.line_vec(self.win.cursor.line)[..self.win.cursor.col]
            .iter()
            .all(|&c| c == ' ' || c == '\t')
    }

    /// Virtual column of char index `col` in `line` (its start).
    fn vcol_of(line: &[char], col: usize, ts: usize) -> usize {
        line[..col]
            .iter()
            .fold(0, |vcol, &c| vcol + text::char_width(c, vcol, ts))
    }

    /// Vim's `ins_tab()` for 'expandtab', 'softtabstop' and 'smarttab'.
    /// Returns false when the Tab is an ordinary character.
    fn ins_tab(&mut self) -> bool {
        let Pos { line, col } = self.win.cursor;
        let ts = self.opts.tabstop();
        let sw = self.opts.shiftwidth();
        let sts = self.opts.softtabstop();
        let et = self.opts.expandtab();
        let sta = self.opts.smarttab();
        let ind = self.in_indent();
        if !et && !(sta && ind && sw != ts) && sts == 0 {
            return false;
        }
        // Spaces up to the next stop: 'shiftwidth' in the indent with
        // 'smarttab', else 'softtabstop', else 'tabstop'.
        let stop = if sta && ind {
            sw
        } else if sts != 0 {
            sts
        } else {
            ts
        };
        let vcol = Self::vcol_of(&self.line_vec(line), col, ts);
        let mode = self.mode;
        for i in 0..stop - vcol % stop {
            if i == 1 {
                // Replace mode replaces one character only.
                self.mode = Mode::Insert;
            }
            self.insert_edit(Key::Char(' '));
        }
        self.mode = mode;
        // Replace mode is left as is: the replace stack (for BS) assumes one
        // entry per typed character, and Vim's rewrite is not reproduced there.
        if !et && (sts != 0 || (sta && ind)) && self.mode != Mode::Replace {
            self.tabify_before_cursor();
        }
        true
    }

    /// The end of Vim's `ins_tab()`: the white space before the cursor is
    /// written with Tabs wherever a Tab stop is reached within it.
    fn tabify_before_cursor(&mut self) {
        let Pos { line, col } = self.win.cursor;
        let ts = self.opts.tabstop();
        let v = self.line_vec(line);
        let mut start = col;
        while start > 0 && matches!(v[start - 1], ' ' | '\t') {
            start -= 1;
        }
        if start == col {
            return;
        }
        let end_vcol = Self::vcol_of(&v, col, ts);
        let mut vcol = Self::vcol_of(&v, start, ts);
        let mut new: Vec<char> = Vec::new();
        while (vcol / ts + 1) * ts <= end_vcol {
            new.push('\t');
            vcol = (vcol / ts + 1) * ts;
        }
        new.extend(std::iter::repeat_n(' ', end_vcol - vcol));
        if new[..] == v[start..col] {
            return;
        }
        self.doc.buf.remove(line, start, col - start);
        let s: String = new.iter().collect();
        self.doc.buf.insert(line, start, &s);
        self.win.cursor.col = start + new.len();
    }

    /// Whether a BS in Insert mode is handled by 'softtabstop' or
    /// 'smarttab' (the condition of Vim's `ins_bs()`).
    fn bs_uses_sts(&self) -> bool {
        let Pos { line, col } = self.win.cursor;
        if col == 0 {
            return false;
        }
        if self.opts.smarttab() && self.in_indent() {
            return true;
        }
        let prev = self.doc.buf.char_at(line, col - 1);
        self.opts.softtabstop() != 0
            && (prev == Some('\t') || (prev == Some(' ') && !self.inserted_space))
    }

    /// Vim's `ins_bs()` with 'softtabstop' or 'smarttab': BS deletes white
    /// space back to the previous stop, then fills up to it with spaces.
    fn ins_bs_sts(&mut self) {
        let Pos { line, mut col } = self.win.cursor;
        let ts = self.opts.tabstop();
        let step = if self.opts.smarttab() && self.in_indent() {
            self.opts.shiftwidth()
        } else {
            self.opts.softtabstop()
        };
        self.u_save_cursor();
        let vcol_at = |buf: &crate::buffer::Buffer, col: usize| {
            Self::vcol_of(&buf.line(line).chars().collect::<Vec<_>>(), col, ts)
        };
        let start_vcol = vcol_at(&self.doc.buf, col);
        // The last cell of the character before the cursor.
        let want = ((start_vcol - 1) / step) * step;
        let mut vcol = start_vcol;
        while vcol > want
            && col > 0
            && matches!(self.doc.buf.char_at(line, col - 1), Some(' ' | '\t'))
        {
            self.doc.buf.remove(line, col - 1, 1);
            col -= 1;
            vcol = vcol_at(&self.doc.buf, col);
        }
        while vcol < want {
            self.doc.buf.insert(line, col, " ");
            col += 1;
            vcol = vcol_at(&self.doc.buf, col);
        }
        if vcol >= start_vcol && col > 0 {
            self.doc.buf.remove(line, col - 1, 1);
            col -= 1;
        }
        self.win.cursor.col = col;
    }

    fn insert_move(&mut self, key: Key) {
        match key {
            Key::Left if self.win.cursor.col > 0 => self.win.cursor.col -= 1,
            Key::Right if self.win.cursor.col < self.line_len() => self.win.cursor.col += 1,
            Key::Up | Key::Down => {
                self.win.keep_curswant = true;
                let ok = if key == Key::Down {
                    self.cursor_down_n(1)
                } else {
                    self.cursor_up_n(1)
                };
                if ok {
                    self.coladvance(self.win.curswant);
                } else {
                    self.beep();
                }
            }
            Key::Home => self.win.cursor.col = 0,
            Key::End => self.win.cursor.col = self.line_len(),
            _ => self.beep(),
        }
    }

    fn finish_insert(&mut self) {
        let keys = std::mem::take(&mut self.ins_keys);
        for _ in 1..self.ins_count {
            if self.ins_kind == InsertKind::OpenLine {
                let at = self.win.cursor.line + 1;
                self.u_save(at, 0, false);
                self.doc.uline = None;
                self.doc.buf.insert_line(at, "");
                self.win.cursor = Pos::new(at, 0);
                self.auto_indent(at, at - 1);
            }
            for &k in &keys {
                self.insert_edit(k);
            }
        }
        self.ins_count = 1;
        if std::mem::take(&mut self.did_ai) {
            // Nothing was typed after an automatic indent: remove it.
            let l = self.win.cursor.line;
            let mut len = self.doc.buf.line_len(l);
            if self.win.cursor.col >= len {
                while len > 0 && matches!(self.doc.buf.char_at(l, len - 1), Some(' ' | '\t')) {
                    self.doc.buf.remove(l, len - 1, 1);
                    len -= 1;
                }
                self.win.cursor.col = len;
            }
        }
        self.mode = Mode::Normal;
        if self.win.cursor.col > 0 {
            self.win.cursor.col -= 1;
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

    // ---- files ----------------------------------------------------------

    /// Replaces the buffer with `path`, reporting like Vim's `:e`.
    pub(crate) fn edit_file(&mut self, path: &Path) {
        match Buffer::load(path) {
            Ok((buf, stats)) => {
                self.doc = Document::new(buf);
                self.win.cursor = Pos::default();
                self.win.curswant = 0;
                self.win.keep_curswant = false;
                // The jumplist and the context mark refer to the old text.
                self.jumps = JumpList::default();
                let name = path.display();
                match stats {
                    None => self.info(format!("\"{name}\" [New]")),
                    Some(s) => self.info(format!("\"{name}\" {}", describe(s, self.doc.buf.crlf))),
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

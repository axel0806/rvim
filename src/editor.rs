// SPDX-License-Identifier: Vim
// Derived in part from Vim (https://www.vim.org); see LICENSE-VIM.
// This is a modified version, not the official Vim.
//! The editor state machine: modes, Insert/Replace mode, the command line,
//! and the glue to undo, registers and `.`. Normal and Visual mode commands
//! are in normal.rs, operators in ops.rs. No terminal dependency: tests
//! drive it with key strings.

use crate::buffer::{Buffer, FileStats, read_file};
use crate::document::{DocId, Document, ULine};
use crate::jumplist::JumpList;
use crate::key::{Key, parse_keys};
use crate::motion::{self, Pos};
use crate::ops::OpArg;
use crate::options::Options;
use crate::register::Registers;
use crate::reload::{
    Change, Decision, DiskState, LineMap, Stale, Warned, external_change_decision,
};
use crate::text;
use crate::undo::{Step, UndoSource};
use crate::watch::{WatchCmd, WatchTarget};
use crate::window::{WinId, Window};
use std::path::Path;
use std::time::SystemTime;

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
    /// Every document, indexed by `DocId` (PLAN.md, "Document, Window, Editor").
    /// Ids are stable; documents are not removed yet.
    documents: Vec<Document>,
    /// Every window, indexed by `WinId`.
    windows: Vec<Window>,
    /// The window with the focus. Commands act on its document and cursor.
    pub(crate) cur_win: WinId,
    pub(crate) mode: Mode,
    /// Keys of a Normal/Visual command still being typed.
    pub(crate) pending: Vec<Key>,
    ins_kind: InsertKind,
    ins_count: usize,
    ins_keys: Vec<Key>,
    ins_stack: Vec<RepItem>,
    pub(crate) ins_post: PostInsert,
    arrow_used: bool,
    /// Where the current Insert started (Vim's `Insstart`), which `'[` is
    /// set to when it stops.
    ins_start: Pos,
    pub(crate) regs: Registers,
    pub(crate) last_find: Option<(char, bool, bool)>,
    pub(crate) search: crate::search::SearchState,
    /// The last pattern used by `/` or `:s` (Vim's `RE_LAST`).
    pub(crate) last_pat: Option<String>,
    /// Set by `:nohlsearch`: 'hlsearch' is not shown until the next search
    /// or `:s` (Vim's `no_hlsearch`).
    pub(crate) no_hlsearch: bool,
    /// The view saved when a `/` or `?` prompt opened with 'incsearch' on,
    /// and the match shown for the text typed so far (see search.rs).
    pub(crate) incsearch: Option<crate::search::IncState>,
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
    /// The terminal size in cells (Vim's `Columns` and `Rows`), set by
    /// `set_screen_size`. The last row is the command line. Each window's text
    /// area is derived from it in `fit_windows`.
    pub(crate) screen_cols: usize,
    pub(crate) screen_rows: usize,
    /// The 'scroll' option for CTRL-D and CTRL-U; 0 means half the window.
    pub(crate) scroll_opt: usize,
    pub(crate) depth: usize,
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
    /// Problems found in the config file at startup (see `config.rs`), for
    /// `:ConfigWarnings`.
    pub(crate) config_warnings: Vec<String>,
    /// The agent layer is on for this session: the config's opt-in, or
    /// `:AgentScan` (see `agent_scan.rs`). Off, the status line is Vim's.
    pub(crate) agent_active: bool,
    /// The last scan's result, for the status indicator.
    pub(crate) agent_scan: Option<crate::agent_detect::ScanSummary>,
    /// Agent requests not yet taken by `take_agent_commands`.
    pub(crate) agent_outbox: Vec<crate::agent_scan::AgentCmd>,
    /// The agent connection (`agent_session.rs`): its link, consent, approvals.
    pub(crate) agent: crate::agent_session::AgentState,
    pub(crate) message: Option<Message>,
    pub(crate) quit: bool,
    bell: bool,
    /// The watch set last sent to the watcher thread (see
    /// `take_watch_commands`). Empty while the option `filewatch` is off.
    watch_sent: Vec<WatchTarget>,
    /// Commands for the watcher thread not yet taken by `take_watch_commands`.
    watch_outbox: Vec<WatchCmd>,
    /// A change the watcher reported that waits for Normal mode (PLAN.md,
    /// "Reload as one undo step": applied on return to Normal mode).
    watch_queue: Option<WatchQueued>,
    /// A modified document whose file changed on disk, waiting for the
    /// conflict prompt's answer (`conflict.rs`). Only with the agent opt-in.
    pub(crate) conflict: Option<crate::conflict::Conflict>,
    /// The agent opt-in (`[agent] enabled` or `--agent`), off by default. It
    /// turns on the conflict prompt and the write guard (`conflict.rs`).
    pub(crate) agent_enabled: bool,
    /// The review gutter's worker jobs and the way back from `[base]`
    /// (review.rs).
    pub(crate) review: crate::review::ReviewState,
}

/// What the watcher reported about a file that is there.
enum Arrival {
    /// The file's state and contents.
    Read(DiskState, Vec<u8>),
    /// The file is over the read cap and its contents were not read.
    TooLarge {
        mtime: Option<SystemTime>,
        size: usize,
    },
}

/// A file change the watcher reported, held until the editor can apply it.
struct WatchQueued {
    doc: DocId,
    version: u64,
    /// What the file is now, or None when it is gone.
    now: Option<Arrival>,
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
        let opts = Options::default();
        let mut ed = Self {
            documents: vec![Document::new(buf, &opts)],
            windows: vec![Window::new(DocId(0), &opts)],
            cur_win: WinId(0),
            mode: Mode::Normal,
            pending: Vec::new(),
            ins_kind: InsertKind::Inline,
            ins_count: 1,
            ins_keys: Vec::new(),
            ins_stack: Vec::new(),
            ins_post: PostInsert::None,
            arrow_used: false,
            ins_start: Pos::default(),
            regs: Registers::new(),
            last_find: None,
            search: Default::default(),
            last_pat: None,
            no_hlsearch: false,
            incsearch: None,
            last_sub: String::new(),
            cmd_search: None,
            last_sub_raw: String::new(),
            sub_flags: Default::default(),
            sub: None,
            ex_tasks: Vec::new(),
            opts,
            did_ai: false,
            inserted_space: false,
            global_busy: false,
            cmd_literal: false,
            global_need_beginline: false,
            redo: None,
            redo_building: None,
            replaying_visual: None,
            resel: None,
            // Vim's default screen when it runs without a terminal.
            screen_cols: 80,
            screen_rows: 24,
            scroll_opt: 0,
            depth: 0,
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
            config_warnings: Vec::new(),
            agent_active: false,
            agent_scan: None,
            agent_outbox: Vec::new(),
            agent: Default::default(),
            message: None,
            quit: false,
            bell: false,
            watch_sent: Vec::new(),
            watch_outbox: Vec::new(),
            watch_queue: None,
            conflict: None,
            agent_enabled: false,
            review: Default::default(),
        };
        ed.fit_windows();
        ed
    }

    /// Document `id`, for code that works on a document other than the one
    /// the current window shows (the conflict prompt, `conflict.rs`).
    pub(crate) fn document(&self, id: DocId) -> &Document {
        &self.documents[id.0]
    }

    pub(crate) fn document_mut(&mut self, id: DocId) -> &mut Document {
        &mut self.documents[id.0]
    }

    /// How many documents there are (ids are `0..document_count()`).
    pub(crate) fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// A new document for `buf`, with the global option values (the agent
    /// log, agent_turn.rs). Documents are not removed yet.
    pub(crate) fn new_document(&mut self, buf: crate::buffer::Buffer) -> DocId {
        self.documents.push(Document::new(buf, &self.opts));
        DocId(self.documents.len() - 1)
    }

    /// Every document's id (the served file calls, `agent_fs.rs`).
    pub(crate) fn document_ids(&self) -> impl Iterator<Item = DocId> + use<> {
        (0..self.documents.len()).map(DocId)
    }

    /// Gives every window the text area of the screen. There is one window,
    /// which fills the screen. Splits (milestone 5) will divide it.
    pub(crate) fn fit_windows(&mut self) {
        let width = self.screen_cols;
        let height = self.screen_rows.saturating_sub(1).max(1);
        for win in &mut self.windows {
            win.width = width;
            win.height = height;
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
            self.info(crate::NOTICE_SHORT);
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn cursor(&self) -> Pos {
        self.win().cursor
    }

    /// The text of the document shown in the current window.
    pub fn buf(&self) -> &Buffer {
        &self.doc().buf
    }

    /// The text of the document shown in the current window, for changes.
    pub(crate) fn buf_mut(&mut self) -> &mut Buffer {
        &mut self.doc_mut().buf
    }

    /// The current window.
    pub(crate) fn win(&self) -> &Window {
        &self.windows[self.cur_win.0]
    }

    pub(crate) fn win_mut(&mut self) -> &mut Window {
        &mut self.windows[self.cur_win.0]
    }

    /// The document shown in the current window.
    pub(crate) fn doc(&self) -> &Document {
        &self.documents[self.win().doc.0]
    }

    pub(crate) fn doc_mut(&mut self) -> &mut Document {
        let id = self.win().doc;
        &mut self.documents[id.0]
    }

    /// The current document and window at once. They live in different
    /// Vecs, so both can be borrowed mutably.
    pub(crate) fn parts_mut(&mut self) -> (&mut Document, &mut Window) {
        let win = &mut self.windows[self.cur_win.0];
        let doc = &mut self.documents[win.doc.0];
        (doc, win)
    }

    /// The other end of the Visual area, when the area is shown.
    pub fn visual_start(&self) -> Option<Pos> {
        self.shown_visual().map(|_| self.win().vstart)
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
        self.refresh_review();
    }

    /// The single entry point for everything that happens to the editor.
    pub fn handle_event(&mut self, ev: crate::event::Event) {
        use crate::event::Event;
        match ev {
            Event::Key(k) => self.handle_key(k),
            Event::Paste(text) => self.paste(&text),
            Event::Resize(cols, rows) => self.set_screen_size(cols as usize, rows as usize),
            Event::FocusGained | Event::FocusLost | Event::Signal(_) => {}
            Event::Timer => self.agent_timer(std::time::Instant::now()),
            Event::AgentScan(scan) => self.agent_arrived(scan),
            Event::AgentConn { generation, event } => self.agent_conn_event(generation, event),
            Event::FileChanged {
                doc,
                version,
                bytes,
                mtime,
                size,
                hash,
            } => {
                // A served agent write is applied by itself, once
                // (`agent_fs.rs`); the watcher's report of it is dropped.
                if !self.agent_fs_echo(doc, hash) {
                    let state = DiskState { mtime, size, hash };
                    self.watch_arrived(doc, version, Some(Arrival::Read(state, bytes)));
                }
            }
            Event::FileDeleted { doc, version } => self.watch_arrived(doc, version, None),
            Event::FileTooLarge {
                doc,
                version,
                mtime,
                size,
            } => self.watch_arrived(doc, version, Some(Arrival::TooLarge { mtime, size })),
            Event::ReviewDiff {
                doc,
                version,
                base_gen,
                hunks,
            } => self.review_arrived(doc, version, base_gen, hunks),
        }
        self.apply_watch_queue();
        self.apply_served_writes();
        self.refresh_review();
    }

    /// The watcher commands the front end must send now (see `watch.rs`):
    /// the resyncs the editor asked for, and the watched set when it has
    /// changed. With the option `filewatch` off the set is empty, so no
    /// watcher command is ever produced, and the watcher thread does nothing.
    pub fn take_watch_commands(&mut self) -> Vec<WatchCmd> {
        let mut out = std::mem::take(&mut self.watch_outbox);
        let want: Vec<WatchTarget> = if self.eff().filewatch() {
            self.documents
                .iter()
                .enumerate()
                .filter_map(|(i, d)| {
                    d.buf.path.as_ref().map(|path| WatchTarget {
                        doc: DocId(i),
                        path: path.clone(),
                        synced: d.disk.clone(),
                        version: d.version(),
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        if want != self.watch_sent {
            self.watch_sent = want.clone();
            out.push(WatchCmd::Set(want));
        }
        out
    }

    /// When the front end should send [`Event::Timer`](crate::event::Event::Timer)
    /// if nothing else happens first. `None` means no timer is needed (there
    /// are no mappings with `timeoutlen` yet).
    pub fn next_deadline(&self) -> Option<std::time::Instant> {
        self.agent_deadline()
    }

    pub fn handle_key(&mut self, key: Key) {
        self.handle_key_inner(key);
        // A queued permission request becomes the prompt when the key left
        // the editor in plain Normal mode (agent_permission.rs). Keys fed
        // from inside a command (macros, `:normal`, `.`) run at depth > 0
        // and never activate it.
        if self.depth == 0 {
            self.permission_activate();
        }
    }

    fn handle_key_inner(&mut self, key: Key) {
        // The conflict prompt takes every key while it is open (conflict.rs).
        if self.conflict.is_some() {
            return self.conflict_key(key);
        }
        // The start menu takes its own keys; any other key closes it and is
        // handled below as usual.
        if self.start_menu_key(key) {
            return;
        }
        // The agent consent prompt takes every key while it is up.
        if self.agent_consent_key(key) {
            return;
        }
        // So does the permission prompt (agent_permission.rs), once it is
        // active: in plain Normal mode only, never in Insert, Replace, Visual
        // or command-line mode or with a Normal command pending.
        if self.agent_permission_key(key) {
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
                // `<Esc>` then the key, as one key: a permission prompt must
                // not become active between the two.
                self.handle_key_inner(Key::Esc);
                return self.handle_key_inner(Key::Char(c));
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
        self.win_mut().keep_curswant = false;
        match self.mode {
            Mode::Normal | Mode::Visual(_) => self.normal_key(key),
            Mode::Insert | Mode::Replace => self.insert_key(key),
            Mode::CmdLine => {
                self.cmdline_key(key);
                self.incsearch_update();
            }
            Mode::Confirm => self.confirm_key(key),
        }
        match self.mode {
            Mode::Normal => self.clamp_normal(),
            Mode::Visual(_) => {
                self.win_mut().cursor.line =
                    self.win_mut().cursor.line.min(self.buf_mut().last_line());
                self.win_mut().cursor.col = self
                    .win_mut()
                    .cursor
                    .col
                    .min(self.buf().line_len(self.win().cursor.line));
            }
            _ => {}
        }
        if !self.win().keep_curswant {
            self.win_mut().curswant = self.cursor_vcol();
        }
        self.update_topline();
        self.sync_jumps();
        // Vim's checkpcmark() at the end of each command. Insert and command
        // line mode are part of the command that started them, so it runs
        // when they end.
        if matches!(self.mode, Mode::Normal | Mode::Visual(_)) && self.pending.is_empty() {
            let cursor = self.win().cursor;
            self.win_mut().jumps.check_pcmark(cursor);
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
        // Pasted text is data, not an answer to the conflict prompt or to the
        // agent consent prompt (a pasted `y` would approve a connection), nor
        // to a permission request.
        if self.conflict.is_some() || self.agent.consent.is_some() || self.permission_prompt_up() {
            return self.beep();
        }
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
                self.incsearch_update();
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
        self.buf().line_len(self.win().cursor.line)
    }

    /// The cursor's virtual column as Vim shows it in the current mode.
    pub fn cursor_vcol(&self) -> usize {
        // Vim's getvvcol(): on a TAB the cursor is at the end of the TAB in
        // Normal mode, but at its start in Visual mode when the cursor is not
        // after the start of the area (LTOREQ_POS in Vim).
        let visual_start = self.is_visual() && self.win().cursor <= self.win().vstart;
        text::cursor_vcol(
            self.buf().line(self.win().cursor.line),
            self.win().cursor.col,
            matches!(self.mode, Mode::Insert | Mode::Replace) || visual_start,
            self.eff().tabstop(),
        )
    }

    /// The cursor's virtual column with the Normal-mode rule for a TAB, even
    /// in Visual mode: Vim's `nv_visual()` reselects with it before the
    /// area's start counts (`3v` after a delete on a TAB).
    pub(crate) fn cursor_vcol_as_normal(&self) -> usize {
        text::cursor_vcol(
            self.buf().line(self.win().cursor.line),
            self.win().cursor.col,
            false,
            self.eff().tabstop(),
        )
    }

    fn clamp_normal(&mut self) {
        self.win_mut().cursor.line = self.win_mut().cursor.line.min(self.buf_mut().last_line());
        let len = self.line_len();
        if self.win().cursor.col >= len {
            self.win_mut().cursor.col = len.saturating_sub(1);
        }
    }

    pub(crate) fn goto_line_first_non_blank(&mut self, line: usize) {
        self.win_mut().keep_curswant = false;
        self.win_mut().cursor.line = line.min(self.buf_mut().last_line());
        self.win_mut().cursor.col =
            motion::first_non_blank(self.buf(), self.win().cursor.line, true);
    }

    pub(crate) fn file_info(&mut self) {
        let name = self.display_name();
        let lines = self.buf().line_count();
        let pct = (self.win().cursor.line + 1) * 100 / lines;
        let modified = if self.buf().modified {
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
        if let Some(name) = self.doc().scratch_name {
            return name.to_string();
        }
        self.buf()
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
        let (doc, win) = self.parts_mut();
        doc.undo.save(
            first,
            count,
            doc.buf.line_count(),
            win.cursor,
            doc.buf.modified,
            doc.buf.marks,
        );
    }

    /// Sets mark `c` at `pos`. An uppercase (file) mark is first removed
    /// from every document, since Vim keeps one mark per letter.
    pub(crate) fn set_named_mark(&mut self, c: char, pos: Pos) {
        self.clear_file_mark(c);
        self.buf_mut().set_mark(c, pos);
    }

    /// Unsets file mark `c` in every document (`:delmarks A`, and before a
    /// file mark is set). Lowercase marks are left alone.
    pub(crate) fn clear_file_mark(&mut self, c: char) {
        if c.is_ascii_uppercase() {
            for d in self.documents.iter_mut() {
                d.buf.set_mark_unset(c);
            }
        }
    }

    pub(crate) fn u_save_cursor(&mut self) {
        self.u_save(self.win().cursor.line, 1, true);
    }

    fn u_saveline(&mut self, l: usize) {
        if self.doc().uline.as_ref().is_some_and(|u| u.line == l) || l >= self.buf().line_count() {
            return;
        }
        self.doc_mut().uline = Some(ULine {
            line: l,
            text: self.buf().line_string(l),
            col: if self.win().cursor.line == l {
                self.win().cursor.col
            } else {
                0
            },
        });
    }

    /// Ends the current undoable change (Vim's `u_sync`).
    pub(crate) fn sync_undo(&mut self) {
        // An empty change (e.g. "x" on an empty line) is still a step, but
        // only real edits set 'modified' (Vim's changed()).
        if self.doc().undo.is_open() {
            let snap = self.buf().snapshot();
            self.doc_mut().undo.sync(snap);
        }
    }

    /// The buffer was written.
    pub(crate) fn mark_saved(&mut self) {
        self.sync_undo();
        self.doc_mut().undo.mark_saved();
        self.buf_mut().modified = false;
    }

    /// `u` and `<C-r>`.
    pub(crate) fn undo_cmd(&mut self, count: usize, redo: bool) {
        self.sync_undo();
        let lines_before = self.buf().line_count() as i64;
        let mut done = 0;
        for _ in 0..count {
            let modified = self.buf().modified;
            let marks = self.buf().marks;
            let step = if redo {
                self.doc_mut().undo.redo(modified, marks)
            } else {
                self.doc_mut().undo.undo(modified, marks)
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
                self.buf_mut().adjust_marks(step.top, oldsize, newsize);
            }
            for (slot, saved) in self.buf_mut().marks.iter_mut().zip(step.marks) {
                if saved.is_some() {
                    *slot = saved;
                }
            }
            // Vim's u_undoredo() sets `'[` and `']` for each step, to the
            // first line above the change and the last line it restored, at
            // column 0 and within the text. The last step's marks are kept.
            let count_before = self.buf().line_count();
            self.buf_mut().restore(&step.snap);
            self.buf_mut().modified = step.modified;
            let lines_after = self.buf().line_count();
            let start = (step.top + 1).min(count_before).min(lines_after);
            let end = if newsize == 0 {
                step.top + 1
            } else {
                step.top + newsize
            }
            .min(lines_after);
            self.doc_mut().op_start = Some(Pos::new(start - 1, 0));
            self.doc_mut().op_end = Some(Pos::new(end - 1, 0));
            self.win_mut().cursor = self.undo_cursor(&step);
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
        let diff = self.buf().line_count() as i64 - lines_before;
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
            self.doc().undo.seq() + usize::from(!redo)
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
        let mut cursor = self.win().cursor;
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
        if cursor.line < self.buf().line_count() {
            if uh.line == cursor.line {
                cursor.col = uh.col;
            } else {
                cursor.col = motion::first_non_blank(self.buf(), cursor.line, true);
            }
        } else {
            cursor.line = self.buf().last_line();
            cursor.col = 0;
        }
        cursor
    }

    /// `U`: restores the last changed line (Vim's `u_undoline()`).
    pub(crate) fn undo_line(&mut self) {
        let Some(u) = self
            .doc()
            .uline
            .clone()
            .filter(|u| u.line < self.buf().line_count())
        else {
            self.beep();
            return;
        };
        self.u_save(u.line, 1, false);
        let old = self.buf().line_string(u.line);
        self.buf_mut().set_line(u.line, &u.text);
        let col = if self.win().cursor.line == u.line {
            self.win().cursor.col
        } else {
            u.col
        };
        self.doc_mut().uline = Some(ULine {
            line: u.line,
            text: old,
            col,
        });
        self.win_mut().cursor = Pos::new(u.line, u.col);
        let len = self.line_len();
        if self.win().cursor.col >= len {
            self.win_mut().cursor.col = len.saturating_sub(1);
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
        self.ins_start = self.win().cursor;
        self.message = None;
    }

    /// Gives line `new` the indent of line `from` when 'autoindent' is on
    /// (the start of Vim's `open_line()`), leaving the cursor after it.
    pub(crate) fn auto_indent(&mut self, new: usize, from: usize) {
        if !self.eff().autoindent() {
            return;
        }
        let ind = Self::get_indent(&self.line_vec(from), self.eff().tabstop());
        if ind > 0 {
            self.set_indent(new, ind);
            let w = self
                .line_vec(new)
                .iter()
                .take_while(|&&c| c == ' ' || c == '\t')
                .count();
            self.win_mut().cursor = Pos::new(new, w);
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
            // Vim's stop_arrow: "." now repeats only what is typed from here,
            // and the insert starts again at the cursor.
            self.arrow_used = false;
            self.ins_start = self.win().cursor;
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
        let edited = self.insert_edit(key);
        // Backspacing over a line break before the start of the insert moves
        // the start back to the join (Vim's ins_bs() does this for the line;
        // on one line `'[` stays where the insert started).
        if key == Key::Backspace && self.win().cursor.line < self.ins_start.line {
            self.ins_start = self.win().cursor;
        }
        if edited {
            self.ins_keys.push(key);
            if let Some(r) = &mut self.redo_building {
                r.keys.push(key);
            }
        }
    }

    /// Applies an editing key. Returns false for keys that do nothing.
    fn insert_edit(&mut self, key: Key) -> bool {
        let Pos { line, col } = self.win().cursor;
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
                    let old = self.buf().char_at(line, col).unwrap_or(' ');
                    self.buf_mut().remove(line, col, 1);
                    self.buf_mut().insert(line, col, c.encode_utf8(&mut [0; 4]));
                    self.ins_stack.push(RepItem::Replaced(old));
                } else {
                    self.buf_mut().insert(line, col, c.encode_utf8(&mut [0; 4]));
                    self.ins_stack.push(RepItem::Appended);
                }
                self.win_mut().cursor.col += 1;
            }
            Key::Enter | Key::Ctrl('j') => {
                self.u_save_cursor();
                self.doc_mut().uline = None;
                let ai = self.eff().autoindent();
                let ind = if ai {
                    // Only the indent before the cursor is copied.
                    Self::get_indent(&self.line_vec(line)[..col], self.eff().tabstop())
                } else {
                    0
                };
                let was_ai = std::mem::take(&mut self.did_ai);
                self.buf_mut().insert(line, col, "\n");
                if ai {
                    // Text after the cursor loses its leading white space,
                    // and an indent that was never used is removed.
                    let skip = self
                        .line_vec(line + 1)
                        .iter()
                        .take_while(|&&c| c == ' ' || c == '\t')
                        .count();
                    if skip > 0 {
                        self.buf_mut().remove(line + 1, 0, skip);
                    }
                    if was_ai {
                        let mut len = self.buf().line_len(line);
                        while len > 0
                            && matches!(self.buf().char_at(line, len - 1), Some(' ' | '\t'))
                        {
                            self.buf_mut().remove(line, len - 1, 1);
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
                self.win_mut().cursor = Pos::new(line + 1, w);
                self.ins_stack.push(RepItem::Newline);
            }
            Key::Backspace if replace => match self.ins_stack.pop() {
                Some(RepItem::Replaced(c)) => {
                    self.u_save_cursor();
                    self.win_mut().cursor.col -= 1;
                    self.buf_mut().remove(line, col - 1, 1);
                    self.buf_mut()
                        .insert(line, col - 1, c.encode_utf8(&mut [0; 4]));
                }
                Some(RepItem::Appended) => {
                    self.u_save_cursor();
                    self.win_mut().cursor.col -= 1;
                    self.buf_mut().remove(line, col - 1, 1);
                }
                Some(RepItem::Newline) => {
                    let prev_len = self.buf().line_len(line - 1);
                    self.u_save(line - 1, 2, true);
                    self.buf_mut().remove(line - 1, prev_len, 1);
                    self.win_mut().cursor = Pos::new(line - 1, prev_len);
                }
                None => {
                    if col > 0 {
                        self.win_mut().cursor.col -= 1;
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
                    self.buf_mut().remove(line, col - 1, 1);
                    self.win_mut().cursor.col -= 1;
                } else if line > 0 {
                    let prev_len = self.buf().line_len(line - 1);
                    self.u_save(line - 1, 2, true);
                    self.buf_mut().remove(line - 1, prev_len, 1);
                    self.win_mut().cursor = Pos::new(line - 1, prev_len);
                } else {
                    self.beep();
                    return false;
                }
            }
            Key::Delete => {
                if col < self.line_len() {
                    self.u_save_cursor();
                    self.buf_mut().remove(line, col, 1);
                } else if line < self.buf().last_line() {
                    self.u_save(line, 2, true);
                    self.buf_mut().remove(line, col, 1);
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
        self.line_vec(self.win().cursor.line)[..self.win().cursor.col]
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
        let Pos { line, col } = self.win().cursor;
        let ts = self.eff().tabstop();
        let sw = self.eff().shiftwidth();
        let sts = self.eff().softtabstop();
        let et = self.eff().expandtab();
        let sta = self.eff().smarttab();
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
        let Pos { line, col } = self.win().cursor;
        let ts = self.eff().tabstop();
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
        self.buf_mut().remove(line, start, col - start);
        let s: String = new.iter().collect();
        self.buf_mut().insert(line, start, &s);
        self.win_mut().cursor.col = start + new.len();
    }

    /// Whether a BS in Insert mode is handled by 'softtabstop' or
    /// 'smarttab' (the condition of Vim's `ins_bs()`).
    fn bs_uses_sts(&self) -> bool {
        let Pos { line, col } = self.win().cursor;
        if col == 0 {
            return false;
        }
        if self.eff().smarttab() && self.in_indent() {
            return true;
        }
        let prev = self.buf().char_at(line, col - 1);
        self.eff().softtabstop() != 0
            && (prev == Some('\t') || (prev == Some(' ') && !self.inserted_space))
    }

    /// Vim's `ins_bs()` with 'softtabstop' or 'smarttab': BS deletes white
    /// space back to the previous stop, then fills up to it with spaces.
    fn ins_bs_sts(&mut self) {
        let Pos { line, mut col } = self.win().cursor;
        let ts = self.eff().tabstop();
        let step = if self.eff().smarttab() && self.in_indent() {
            self.eff().shiftwidth()
        } else {
            self.eff().softtabstop()
        };
        self.u_save_cursor();
        let vcol_at = |buf: &crate::buffer::Buffer, col: usize| {
            Self::vcol_of(&buf.line(line).chars().collect::<Vec<_>>(), col, ts)
        };
        let start_vcol = vcol_at(self.buf(), col);
        // The last cell of the character before the cursor.
        let want = ((start_vcol - 1) / step) * step;
        let mut vcol = start_vcol;
        while vcol > want
            && col > 0
            && matches!(self.buf().char_at(line, col - 1), Some(' ' | '\t'))
        {
            self.buf_mut().remove(line, col - 1, 1);
            col -= 1;
            vcol = vcol_at(self.buf(), col);
        }
        while vcol < want {
            self.buf_mut().insert(line, col, " ");
            col += 1;
            vcol = vcol_at(self.buf(), col);
        }
        if vcol >= start_vcol && col > 0 {
            self.buf_mut().remove(line, col - 1, 1);
            col -= 1;
        }
        self.win_mut().cursor.col = col;
    }

    fn insert_move(&mut self, key: Key) {
        match key {
            Key::Left if self.win_mut().cursor.col > 0 => self.win_mut().cursor.col -= 1,
            Key::Right if self.win_mut().cursor.col < self.line_len() => {
                self.win_mut().cursor.col += 1
            }
            Key::Up | Key::Down => {
                self.win_mut().keep_curswant = true;
                let ok = if key == Key::Down {
                    self.cursor_down_n(1)
                } else {
                    self.cursor_up_n(1)
                };
                if ok {
                    self.coladvance(self.win().curswant);
                } else {
                    self.beep();
                }
            }
            Key::Home => self.win_mut().cursor.col = 0,
            Key::End => self.win_mut().cursor.col = self.line_len(),
            _ => self.beep(),
        }
    }

    fn finish_insert(&mut self) {
        let keys = std::mem::take(&mut self.ins_keys);
        for _ in 1..self.ins_count {
            if self.ins_kind == InsertKind::OpenLine {
                let at = self.win().cursor.line + 1;
                self.u_save(at, 0, false);
                self.doc_mut().uline = None;
                self.buf_mut().insert_line(at, "");
                self.win_mut().cursor = Pos::new(at, 0);
                self.auto_indent(at, at - 1);
            }
            for &k in &keys {
                self.insert_edit(k);
            }
        }
        self.ins_count = 1;
        if std::mem::take(&mut self.did_ai) {
            // Nothing was typed after an automatic indent: remove it.
            let l = self.win().cursor.line;
            let mut len = self.buf().line_len(l);
            if self.win().cursor.col >= len {
                while len > 0 && matches!(self.buf().char_at(l, len - 1), Some(' ' | '\t')) {
                    self.buf_mut().remove(l, len - 1, 1);
                    len -= 1;
                }
                self.win_mut().cursor.col = len;
            }
        }
        // Vim's stop_insert(): `'[` is where the insert started and `']` is
        // the cursor before it moves back. Not set when the arrow keys moved
        // the cursor in the insert.
        if !self.arrow_used {
            self.doc_mut().op_start = Some(self.ins_start);
            self.doc_mut().op_end = Some(self.win().cursor);
        }
        self.mode = Mode::Normal;
        if self.win().cursor.col > 0 {
            self.win_mut().cursor.col -= 1;
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

    /// Replaces the buffer with `path`, reporting like Vim's `:e`. The disk
    /// state is taken from the same read as the text.
    pub(crate) fn edit_file(&mut self, path: &Path) {
        match read_file(path) {
            Ok(read) => {
                let (buf, stats) =
                    Buffer::from_file_bytes(path, read.as_ref().map(|(_, b)| b.as_slice()));
                let disk = read
                    .as_ref()
                    .map(|(mtime, bytes)| DiskState::of(*mtime, bytes));
                let lines = buf.lines();
                self.doc_mut().reload(buf);
                self.doc_mut()
                    .set_baseline(disk.as_ref().map(|_| lines), true);
                self.doc_mut().disk = disk;
                self.win_mut().cursor = Pos::default();
                self.win_mut().curswant = 0;
                self.win_mut().keep_curswant = false;
                // The jumplist and the context mark refer to the old text.
                self.win_mut().jumps = JumpList::default();
                let name = path.display();
                match stats {
                    None => self.info(format!("\"{name}\" [New]")),
                    Some(s) => self.info(format!("\"{name}\" {}", describe(s, self.buf().crlf))),
                }
            }
            Err(e) => self.error(format!("E484: Can't open file {}: {e}", path.display())),
        }
    }

    /// Replaces the text of document `doc` with `new`, the file's contents as
    /// `Buffer::from_file_bytes` decodes them, as one undo step tagged
    /// `source` (PLAN.md, "Reload as one undo step"). This is a reload, not
    /// an edit: the cursor and scroll top of each window on the document
    /// follow their lines (`reload.rs`), the marks, the jumplist and the
    /// context mark move with the lines too, and the buffer is not modified
    /// afterwards. `u` goes back to the old text, modified, and `<C-r>` to the
    /// new one, unmodified. The disk state is for the caller to record.
    ///
    /// Returns `None` when the text is the same: only the line ends or the
    /// empty state changed, which is no undo step.
    pub(crate) fn apply_external_change(
        &mut self,
        doc: DocId,
        new: Buffer,
        source: UndoSource,
    ) -> Option<Change> {
        self.apply_text_change(doc, new, source, false)
    }

    /// The change behind `apply_external_change`. `modified` is the buffer's
    /// state after it: a reload leaves the buffer unmodified, and a merge of
    /// the conflict prompt (`conflict.rs`) leaves it modified.
    pub(crate) fn apply_text_change(
        &mut self,
        doc: DocId,
        new: Buffer,
        source: UndoSource,
        modified: bool,
    ) -> Option<Change> {
        let old_lines = self.documents[doc.0].buf.lines();
        let new_lines = new.lines();
        if old_lines == new_lines {
            self.documents[doc.0]
                .buf
                .set_file_flags(new.crlf, new.has_no_lines());
            return None;
        }
        let map = LineMap::new(&old_lines, &new_lines);
        let cursor = {
            let win = &self.windows[self.cur_win.0];
            if win.doc == doc {
                win.cursor
            } else {
                Pos::default()
            }
        };
        let d = &mut self.documents[doc.0];
        // Close the user's open change first, so the reload is a step of its own.
        if d.undo.is_open() {
            let snap = d.buf.snapshot();
            d.undo.sync(snap);
        }
        // The whole span of changed lines is saved, with the cursor from
        // before the reload, as an ordinary change. `modified` is true, so
        // that undoing the reload leaves the buffer different from the file.
        d.undo.save(
            map.top,
            map.old_count,
            old_lines.len(),
            cursor,
            true,
            d.buf.marks,
        );
        d.undo.tag_open(source);
        let remap = map.clone();
        d.buf.replace_text(new, move |p| remap.pos(p, p.col));
        for slot in d
            .buf
            .marks
            .iter_mut()
            .chain(d.buf.file_marks.iter_mut())
            .flatten()
        {
            *slot = map.pos(*slot, slot.col);
        }
        // `'[` and `']` follow the lines like the other marks; an unset one
        // stays unset.
        d.op_start = d.op_start.map(|p| map.pos(p, p.col));
        d.op_end = d.op_end.map(|p| map.pos(p, p.col));
        if let Some(v) = d.last_visual.as_mut() {
            v.start = map.pos(v.start, v.start.col);
            v.end = map.pos(v.end, v.end.col);
        }
        // The `U` line refers to the old text.
        d.uline = None;
        d.buf.modified = modified;
        let snap = d.buf.snapshot();
        d.undo.sync(snap);
        for win in self.windows.iter_mut().filter(|w| w.doc == doc) {
            let (new_line, exact) = map.line(win.cursor.line);
            win.top = map.top(win.top, win.cursor.line, new_line);
            win.cursor = map.pos(win.cursor, win.curswant);
            if exact {
                win.curswant = win.cursor.col;
            }
            win.vstart = map.pos(win.vstart, win.vstart.col);
        }
        // The start of an Insert in progress (Vim's Insstart) follows the lines.
        self.ins_start = map.pos(self.ins_start, self.ins_start.col);
        Some(Change {
            added: map.added,
            removed: map.removed,
            fallback: map.fallback,
        })
    }

    /// Checks the file of document `doc` against the state its buffer last
    /// synced with, and acts on the decision (`external_change_decision`). It
    /// is the one place for `:checktime` and for the watcher, so they cannot
    /// disagree. `now` is the file's state and bytes, None when it is gone.
    /// Returns the E211 text for a gone file, for the caller to show.
    ///
    /// `from_watch` is set by the watcher. A warning (W11, W12, W13, E211)
    /// is then shown once per change: the watcher reports a change once, but
    /// a dropped event may come again, and the same warning is not repeated.
    /// `:checktime` always shows its warning.
    pub(crate) fn check_disk(
        &mut self,
        doc: DocId,
        now: Option<(DiskState, &[u8])>,
        from_watch: bool,
    ) -> Option<String> {
        let path = self.documents[doc.0].buf.path.clone()?;
        let name = path.display().to_string();
        let now_state = now.as_ref().map(|(s, _)| s.clone());
        let bytes: Option<&[u8]> = now.as_ref().map(|(_, b)| *b);
        let autoread = self.autoread_on();
        let d = &self.documents[doc.0];
        let decision = external_change_decision(
            d.disk.as_ref(),
            now_state.as_ref(),
            d.buf.modified,
            autoread,
        );
        let key = match (decision, &now_state) {
            (Decision::Deleted, _) => Some(Warned::Deleted),
            (Decision::Created | Decision::Changed { .. }, Some(s)) => {
                Some(Warned::Changed(s.hash))
            }
            _ => None,
        };
        let repeat = from_watch && key.is_some() && d.warned == key;
        // With the opt-in, a modified buffer whose file changed (and that has
        // not been shown this state with `k`) opens the conflict prompt.
        let opt_in = self.agent_enabled;
        let prompt_seen = match &now_state {
            Some(s)
                if opt_in
                    && matches!(decision, Decision::Changed { modified: true })
                    && d.seen.as_ref() != Some(s) =>
            {
                Some(s.clone())
            }
            _ => None,
        };
        let mut info: Option<String> = None;
        let mut error: Option<String> = None;
        let mut reloaded: Option<Option<Change>> = None;
        let d = &mut self.documents[doc.0];
        match decision {
            Decision::Same => {
                d.stale = None;
                d.warned = None;
            }
            Decision::Touched => {
                // Same contents, a new modification time or size (touched, or
                // written by rvim itself and read back). Not a change: record it.
                d.disk = now_state.clone();
                d.stale = None;
                d.warned = None;
            }
            Decision::Deleted => {
                // A buffer that never had a file on disk has nothing to lose;
                // this one had, so the buffer is kept and marked.
                d.stale = Some(Stale::Deleted);
                d.warned = key;
                if !repeat && opt_in {
                    info = Some(format!("\"{name}\" was deleted on disk"));
                } else if !repeat {
                    error = Some(format!("E211: File \"{name}\" no longer available"));
                }
            }
            Decision::Created => {
                // The file was created after editing started.
                d.stale = Some(Stale::Changed);
                d.warned = key;
                if !repeat {
                    info = Some(format!(
                        "W13: Warning: File \"{name}\" has been created after editing started"
                    ));
                }
            }
            Decision::Changed { modified } => {
                // The buffer is not touched. Its disk state stays as it was,
                // so the change is still seen (and reported again) by the next
                // check.
                d.stale = Some(Stale::Changed);
                d.warned = key;
                if !repeat && prompt_seen.is_none() {
                    let (warning, what) = if modified {
                        (
                            "W12: Warning",
                            "has changed and the buffer was changed in Vim as well",
                        )
                    } else {
                        ("W11: Warning", "has changed since editing started")
                    };
                    info = Some(format!("{warning}: File \"{name}\" {what}"));
                }
            }
            Decision::Reload => {}
        }
        if let Some(seen) = prompt_seen {
            self.open_conflict(doc, seen);
            if !repeat {
                info = Some(crate::conflict::prompt_message(&name));
            }
        }
        if let (Decision::Reload, Some(bytes)) = (decision, bytes) {
            // Unmodified, and 'autoread' is on: reload as one undo step.
            let (new, _) = Buffer::from_file_bytes(&path, Some(bytes));
            let lines = new.lines();
            let change = self.apply_external_change(doc, new, UndoSource::External);
            let d = &mut self.documents[doc.0];
            d.set_baseline(Some(lines), false);
            d.disk = now_state;
            d.stale = None;
            d.warned = None;
            reloaded = Some(change);
        }
        if let Some(change) = reloaded {
            info = Some(crate::conflict::reload_message(&name, change));
        }
        if let Some(text) = info {
            self.info(text);
        }
        error
    }

    /// A change the watcher reported (`Event::FileChanged` or
    /// `Event::FileDeleted`). It is dropped when the text has changed since
    /// the watcher was told the version: the watcher is asked to look again,
    /// so the change is reported again. Otherwise it waits for Normal mode.
    fn watch_arrived(&mut self, doc: DocId, version: u64, now: Option<Arrival>) {
        if !self.eff().filewatch() {
            return;
        }
        let Some(d) = self.documents.get(doc.0) else {
            return;
        };
        if d.version() != version {
            self.watch_outbox.push(WatchCmd::Resync(doc));
            return;
        }
        if !self.ready_for_change() {
            // Tell the user once that it will load in Normal mode, when it
            // would load (unmodified and 'autoread' on).
            let d = &self.documents[doc.0];
            let note = match &d.buf.path {
                Some(path)
                    if !d.buf.modified
                        && matches!(now, Some(Arrival::Read(..)))
                        && self.autoread_on() =>
                {
                    Some(format!(
                        "\"{}\" changed on disk; handled when you return to Normal mode",
                        path.display()
                    ))
                }
                _ => None,
            };
            if let Some(note) = note {
                self.info(note);
            }
        }
        // A newer report replaces an older one still waiting.
        self.watch_queue = Some(WatchQueued { doc, version, now });
    }

    /// Whether a change from outside may be applied now: Normal mode, no
    /// command being typed, and no start menu.
    pub(crate) fn ready_for_change(&self) -> bool {
        self.mode == Mode::Normal && self.pending.is_empty() && self.start_menu.is_none()
    }

    /// Applies the waiting watcher change when the editor is ready for it.
    fn apply_watch_queue(&mut self) {
        if !self.ready_for_change() {
            return;
        }
        let Some(q) = self.watch_queue.take() else {
            return;
        };
        let Some(d) = self.documents.get(q.doc.0) else {
            return;
        };
        if d.version() != q.version {
            // The text changed while the change waited: look again.
            self.watch_outbox.push(WatchCmd::Resync(q.doc));
            return;
        }
        let error = match &q.now {
            Some(Arrival::TooLarge { mtime, size }) => {
                self.check_too_large(q.doc, *mtime, *size);
                None
            }
            Some(Arrival::Read(state, bytes)) => {
                self.check_disk(q.doc, Some((state.clone(), bytes.as_slice())), true)
            }
            None => self.check_disk(q.doc, None, true),
        };
        if let Some(error) = error {
            self.error(error);
        }
    }

    /// A change to a file over the watcher's read cap (`MAX_WATCH_BYTES`).
    /// Its contents are not known, so nothing is compared or loaded: the
    /// buffer is kept and marked stale, and the user is told once to use
    /// `:e!`, which reads the file.
    fn check_too_large(&mut self, doc: DocId, mtime: Option<SystemTime>, size: usize) {
        let Some(path) = self.documents[doc.0].buf.path.clone() else {
            return;
        };
        let name = path.display().to_string();
        let key = Warned::TooLarge { mtime, size };
        let d = &mut self.documents[doc.0];
        let repeat = d.warned == Some(key);
        d.stale = Some(Stale::Changed);
        d.warned = Some(key);
        if !repeat {
            self.info(format!(
                "\"{name}\" changed on disk; too large to reload automatically, use :e!"
            ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn version(ed: &Editor) -> u64 {
        ed.doc().version()
    }

    #[test]
    fn current_window_leads_to_its_document() {
        let ed = Editor::from_text("abc\n");
        assert_eq!(ed.cur_win, WinId(0));
        let doc_id = ed.win().doc;
        assert_eq!(doc_id, DocId(0));
        assert!(std::ptr::eq(ed.win(), &ed.windows[ed.cur_win.0]));
        assert!(std::ptr::eq(ed.doc(), &ed.documents[doc_id.0]));
        assert!(std::ptr::eq(ed.buf(), &ed.documents[doc_id.0].buf));
    }

    #[test]
    fn parts_mut_changes_buffer_and_cursor_in_one_borrow() {
        let mut ed = Editor::from_text("abc\ndef\n");
        {
            let (doc, win) = ed.parts_mut();
            doc.buf.insert(1, 0, "x");
            win.cursor = Pos::new(1, 2);
        }
        assert_eq!(ed.buf().line_string(1), "xdef");
        assert_eq!(ed.cursor(), Pos::new(1, 2));
    }

    #[test]
    fn window_fills_the_screen_above_the_command_line() {
        let mut ed = Editor::from_text("abc\n");
        assert_eq!((ed.win().width, ed.win().height), (80, 23));
        ed.set_screen_size(100, 10);
        assert_eq!((ed.win().width, ed.win().height), (100, 9));
        assert_eq!(ed.text_rows(), 9);
        ed.set_screen_size(0, 0);
        assert_eq!((ed.win().width, ed.win().height), (1, 1));
    }

    #[test]
    fn window_resize_keeps_version() {
        let mut ed = Editor::from_text("abc\ndef\n");
        let v0 = version(&ed);
        ed.set_screen_size(40, 5);
        assert_eq!(ed.win().height, 4);
        assert_eq!(version(&ed), v0);
    }

    fn lines_of(ed: &Editor) -> Vec<String> {
        ed.buf().lines()
    }

    #[test]
    fn external_change_is_one_undo_step_tagged_external() {
        let mut ed = Editor::from_text("one\ntwo\nthree");
        ed.feed("ix<Esc>");
        let before = lines_of(&ed);
        let user_seq = ed.doc().undo.seq();
        let new = Buffer::from_text("one\nTWO\nthree\nfour");
        let v0 = version(&ed);
        let change = ed
            .apply_external_change(DocId::default(), new, UndoSource::External)
            .unwrap();
        assert_eq!(lines_of(&ed), ["one", "TWO", "three", "four"]);
        assert!(!ed.buf().modified, "the buffer equals the file");
        assert!(version(&ed) > v0, "version goes up");
        // "three" is the only line in both; "xone", "two" went and "TWO", "four" came.
        assert_eq!(
            (change.added, change.removed, change.fallback),
            (3, 2, false)
        );
        let seq = ed.doc().undo.seq();
        assert_eq!(seq, user_seq + 1, "one step");
        assert_eq!(ed.doc().undo.source(seq), Some(UndoSource::External));
        assert_eq!(ed.doc().undo.source(user_seq), Some(UndoSource::User));

        // `u` restores the text before the reload in one step; it is modified now.
        ed.feed("u");
        assert_eq!(lines_of(&ed), before);
        assert!(ed.buf().modified);
        assert_eq!(ed.doc().undo.seq(), user_seq);
        // `<C-r>` applies the reload again, and the buffer is unmodified.
        ed.feed("<C-r>");
        assert_eq!(lines_of(&ed), ["one", "TWO", "three", "four"]);
        assert!(!ed.buf().modified);
        // The user's change is still one step below.
        ed.feed("u");
        ed.feed("u");
        assert_eq!(lines_of(&ed), ["one", "two", "three"]);
    }

    #[test]
    fn external_change_keeps_the_cursor_on_its_line() {
        let mut ed = Editor::from_text("one\ntwo\nthree");
        ed.feed("jl");
        // Lines inserted above: the cursor stays on "two", now line 2.
        let new = Buffer::from_text("zero\none\ntwo\nthree");
        ed.apply_external_change(DocId::default(), new, UndoSource::External);
        assert_eq!(ed.cursor(), Pos::new(2, 1));
    }

    #[test]
    fn external_change_moves_the_cursor_off_a_replaced_line_to_the_desired_column() {
        let mut ed = Editor::from_text("one\ntwo\nthree");
        ed.feed("jl");
        assert_eq!(ed.cursor(), Pos::new(1, 1));
        let new = Buffer::from_text("one\nTWO-LONGER\nthree");
        ed.apply_external_change(DocId::default(), new, UndoSource::External);
        // The line was replaced: first new line of the replacement, the
        // desired column (1, set by `l`).
        assert_eq!(ed.cursor(), Pos::new(1, 1));
        // And a replacement that is shorter is clipped to its last character.
        let mut ed = Editor::from_text("one\ntwo\nthree");
        ed.feed("jll");
        let new = Buffer::from_text("one\nX\nthree");
        ed.apply_external_change(DocId::default(), new, UndoSource::External);
        assert_eq!(ed.cursor(), Pos::new(1, 0));
    }

    #[test]
    fn external_change_moves_marks_and_the_jumplist_with_the_lines() {
        let mut ed = Editor::from_text("one\ntwo\nthree\nfour");
        ed.feed("jjma");
        ed.feed("G");
        // G set the context mark to where the cursor was: line 2.
        let new = Buffer::from_text("zero\none\ntwo\nthree\nfour");
        ed.apply_external_change(DocId::default(), new, UndoSource::External);
        assert_eq!(ed.buf().mark('a'), Some(Pos::new(3, 0)));
        // The context mark followed the line too: `''` goes to "two"'s new line.
        ed.feed("''");
        assert_eq!(ed.cursor().line, 3);
    }

    #[test]
    fn external_change_keeps_the_scroll_top_on_its_line() {
        let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
        let mut ed = Editor::from_text(&text);
        ed.set_screen_size(40, 11);
        ed.feed("30G");
        let top = ed.win().top;
        assert!(top > 0);
        let mut lines = ed.buf().lines();
        lines.insert(0, "inserted".to_string());
        let new = Buffer::from_text(&lines.join("\n"));
        ed.apply_external_change(DocId::default(), new, UndoSource::External);
        assert_eq!(ed.win().top, top + 1);
        assert_eq!(ed.cursor().line, 30);
    }

    #[test]
    fn external_change_keeps_line_ends_of_the_new_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crlf.txt");
        std::fs::write(&path, b"one\r\ntwo\r\n").unwrap();
        let mut ed = Editor::open(&path);
        assert!(ed.buf().crlf);
        // The file gets LF endings and one more line.
        let (new, _) = Buffer::from_file_bytes(&path, Some(b"one\ntwo\nthree\n"));
        assert!(!new.crlf);
        ed.apply_external_change(DocId::default(), new, UndoSource::External);
        assert!(!ed.buf().crlf);
        assert_eq!(ed.buf().to_bytes(), b"one\ntwo\nthree\n");
        // Same text with CRLF ends: flags only, no undo step.
        let seq = ed.doc().undo.seq();
        let (new, _) = Buffer::from_file_bytes(&path, Some(b"one\r\ntwo\r\nthree\r\n"));
        assert!(
            ed.apply_external_change(DocId::default(), new, UndoSource::External)
                .is_none()
        );
        assert!(ed.buf().crlf);
        assert_eq!(ed.doc().undo.seq(), seq);
        assert!(!ed.buf().modified);
    }
}

/// The file watcher's handling in the editor (watch.rs does the polling):
/// the version rule, Normal mode, the decision shared with `:checktime`, and
/// the warnings. Events are built as the watcher builds them, from the file.
#[cfg(test)]
mod watch_tests {
    use super::*;
    use crate::event::Event;
    use crate::reload::{Stale, fnv1a};
    use crate::watch::WatchCmd;
    use std::fs;
    use std::path::PathBuf;

    /// `f.txt` in a new temp dir, opened in an editor.
    fn open(dir: &tempfile::TempDir, contents: &str) -> (Editor, PathBuf) {
        let path = dir.path().join("f.txt");
        fs::write(&path, contents).unwrap();
        (Editor::open(&path), path)
    }

    fn message_of(ed: &Editor) -> String {
        ed.message().map(|m| m.text.clone()).unwrap_or_default()
    }

    /// The event the watcher sends for the file as it is on disk now.
    fn event_for_disk(ed: &Editor, path: &Path) -> Event {
        let bytes = fs::read(path).unwrap();
        let mtime = fs::metadata(path).unwrap().modified().ok();
        Event::FileChanged {
            doc: DocId::default(),
            version: ed.doc().version(),
            size: bytes.len(),
            hash: fnv1a(&bytes),
            mtime,
            bytes,
        }
    }

    /// Turns the option on and takes the watcher command it produces.
    fn watch_on(ed: &mut Editor) -> Vec<WatchCmd> {
        ed.feed(":set filewatch<CR>");
        ed.take_watch_commands()
    }

    #[test]
    fn with_the_option_off_nothing_is_watched_and_no_event_changes_the_buffer() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        ed.feed("ix<Esc>:w<CR>");
        assert!(ed.take_watch_commands().is_empty(), "no watched set");
        // A change on disk and an event for it: the option is off, so nothing.
        fs::write(&path, "other\n").unwrap();
        let seq = ed.doc().undo.seq();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), ["xone"]);
        assert_eq!(ed.doc().undo.seq(), seq);
        assert_eq!(ed.doc().stale, None);
        assert!(!message_of(&ed).starts_with('W'), "{}", message_of(&ed));
        assert!(ed.take_watch_commands().is_empty());
    }

    #[test]
    fn turning_the_option_on_sends_the_watched_set_and_only_changes_send_more() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        let cmds = watch_on(&mut ed);
        let [WatchCmd::Set(targets)] = cmds.as_slice() else {
            panic!("{cmds:?}");
        };
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, path);
        assert_eq!(targets[0].synced, ed.doc().disk.clone());
        // Nothing changed: no command.
        assert!(ed.take_watch_commands().is_empty());
        // A text change moves the version the watcher is told about.
        ed.feed("x");
        assert!(matches!(
            ed.take_watch_commands().as_slice(),
            [WatchCmd::Set(_)]
        ));
    }

    #[test]
    fn an_event_for_an_older_version_is_dropped_and_the_file_is_looked_at_again() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        let seen = ed.doc().version();
        ed.feed("x");
        fs::write(&path, "two\n").unwrap();
        let mut stale = event_for_disk(&ed, &path);
        if let Event::FileChanged { version, .. } = &mut stale {
            *version = seen;
        }
        ed.handle_event(stale);
        assert_eq!(ed.buf().lines(), ["ne"], "the text is not replaced");
        assert_eq!(ed.doc().stale, None);
        let cmds = ed.take_watch_commands();
        assert!(
            cmds.contains(&WatchCmd::Resync(DocId::default())),
            "{cmds:?}"
        );
    }

    #[test]
    fn unmodified_with_autoread_reloads_as_one_undo_step_tagged_external() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\ntwo\n");
        watch_on(&mut ed);
        ed.feed(":set autoread<CR>");
        let seq = ed.doc().undo.seq();
        fs::write(&path, "one\nTWO\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), ["one", "TWO"]);
        assert_eq!(ed.doc().undo.seq(), seq + 1, "one step");
        assert_eq!(ed.doc().undo.source(seq + 1), Some(UndoSource::External));
        assert!(
            message_of(&ed).contains("changed on disk"),
            "{}",
            message_of(&ed)
        );
        assert!(!ed.buf().modified);
        assert_eq!(ed.doc().stale, None);
        // The disk state is the new file: the same event again does nothing.
        let seq = ed.doc().undo.seq();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.doc().undo.seq(), seq);
        // And `u` undoes the reload.
        ed.feed("u");
        assert_eq!(ed.buf().lines(), ["one", "two"]);
    }

    #[test]
    fn insert_mode_holds_the_change_until_normal_mode() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        ed.feed(":set autoread<CR>");
        ed.feed("i");
        fs::write(&path, "two\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), ["one"], "not while typing");
        let note = format!(
            "\"{}\" changed on disk; handled when you return to Normal mode",
            path.display()
        );
        assert_eq!(message_of(&ed), note);
        ed.handle_event(Event::Key(Key::Esc));
        assert_eq!(
            ed.buf().lines(),
            ["two"],
            "applied on return to Normal mode"
        );
        assert_eq!(ed.doc().stale, None);
    }

    #[test]
    fn a_command_being_typed_holds_the_change() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\ntwo\n");
        watch_on(&mut ed);
        ed.feed(":set autoread<CR>");
        ed.feed("d");
        assert!(!ed.pending_keys().is_empty());
        fs::write(&path, "new\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(
            ed.buf().lines(),
            ["one", "two"],
            "held while `d` is pending"
        );
        ed.handle_event(Event::Key(Key::Esc));
        assert_eq!(
            ed.buf().lines(),
            ["new"],
            "applied once the command is over"
        );
    }

    #[test]
    fn the_command_line_holds_the_change_too() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        ed.feed(":set autoread<CR>");
        ed.feed(":");
        fs::write(&path, "two\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), ["one"]);
        ed.handle_event(Event::Key(Key::Esc));
        assert_eq!(ed.buf().lines(), ["two"]);
    }

    #[test]
    fn a_modified_buffer_is_kept_and_warned_once() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        ed.feed(":set autoread<CR>");
        ed.feed("ix<Esc>");
        fs::write(&path, "two\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), ["xone"]);
        assert_eq!(ed.doc().stale, Some(Stale::Changed));
        assert!(message_of(&ed).starts_with("W12"), "{}", message_of(&ed));
        // The same change reported again (a resync) is not warned about again.
        ed.message = None;
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(message_of(&ed), "");
        assert_eq!(ed.doc().stale, Some(Stale::Changed));
        assert_eq!(ed.buf().lines(), ["xone"]);
    }

    #[test]
    fn noautoread_keeps_an_unmodified_buffer_and_warns_once() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        fs::write(&path, "two\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), ["one"]);
        assert_eq!(ed.doc().stale, Some(Stale::Changed));
        assert!(message_of(&ed).starts_with("W11"), "{}", message_of(&ed));
        ed.message = None;
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(message_of(&ed), "");
        // The next change is a new warning.
        fs::write(&path, "three\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert!(message_of(&ed).starts_with("W11"), "{}", message_of(&ed));
    }

    #[test]
    fn the_watcher_and_checktime_give_the_same_warning() {
        let dir = tempfile::tempdir().unwrap();
        // Two files in two dirs, so each message names its own path; the
        // paths are replaced by "P" before the messages are compared.
        let dir2 = tempfile::tempdir().unwrap();
        let (mut by_check, path) = open(&dir, "one\n");
        by_check.feed("ix<Esc>");
        fs::write(&path, "two\n").unwrap();
        by_check.feed(":checktime<CR>");

        let (mut by_watch, path2) = open(&dir2, "one\n");
        watch_on(&mut by_watch);
        by_watch.feed("ix<Esc>");
        fs::write(&path2, "two\n").unwrap();
        by_watch.handle_event(event_for_disk(&by_watch, &path2));

        let normal = |ed: &Editor, p: &Path| message_of(ed).replace(&p.display().to_string(), "P");
        assert_eq!(by_watch.doc().stale, by_check.doc().stale);
        assert_eq!(normal(&by_watch, &path2), normal(&by_check, &path));
        assert!(normal(&by_check, &path).starts_with("W12"));
        assert_eq!(by_watch.buf().lines(), by_check.buf().lines());
    }

    #[test]
    fn a_deleted_file_keeps_the_buffer_and_says_so_once() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        fs::remove_file(&path).unwrap();
        let version = ed.doc().version();
        ed.handle_event(Event::FileDeleted {
            doc: DocId::default(),
            version,
        });
        assert_eq!(ed.buf().lines(), ["one"]);
        assert_eq!(ed.doc().stale, Some(Stale::Deleted));
        assert!(message_of(&ed).starts_with("E211"), "{}", message_of(&ed));
        assert!(ed.message().is_some_and(|m| m.error));
        ed.message = None;
        ed.handle_event(Event::FileDeleted {
            doc: DocId::default(),
            version,
        });
        assert_eq!(message_of(&ed), "", "not again");
    }

    #[test]
    fn a_file_created_after_editing_started_is_warned_as_w13() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let mut ed = Editor::open(&path);
        watch_on(&mut ed);
        fs::write(&path, "hello\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), [""], "the empty buffer is kept");
        assert_eq!(ed.doc().stale, Some(Stale::Changed));
        assert!(message_of(&ed).starts_with("W13"), "{}", message_of(&ed));
    }

    #[test]
    fn our_own_write_read_back_is_not_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        ed.feed(":set autoread<CR>");
        ed.feed("ix<Esc>:w<CR>");
        let seq = ed.doc().undo.seq();
        ed.message = None;
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), ["xone"]);
        assert_eq!(ed.doc().undo.seq(), seq, "no step");
        assert_eq!(ed.doc().stale, None);
        assert_eq!(message_of(&ed), "", "no message");
    }

    #[test]
    fn a_file_over_the_read_cap_is_marked_stale_and_warned_once_without_loading() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        ed.feed(":set autoread<CR>");
        let version = ed.doc().version();
        // Even unmodified with 'autoread' on, the text is not replaced.
        let big = |version| Event::FileTooLarge {
            doc: DocId::default(),
            version,
            mtime: None,
            size: crate::watch::MAX_WATCH_BYTES + 1,
        };
        ed.handle_event(big(version));
        assert_eq!(ed.buf().lines(), ["one"], "not loaded");
        assert!(!ed.buf().modified);
        assert_eq!(ed.doc().stale, Some(Stale::Changed));
        assert!(
            message_of(&ed).contains("too large to reload automatically, use :e!"),
            "{}",
            message_of(&ed)
        );
        // The same change again is not warned about again.
        ed.message = None;
        ed.handle_event(big(version));
        assert_eq!(message_of(&ed), "");
        assert_eq!(ed.buf().lines(), ["one"]);
        // `:e!` loads the file as it is.
        fs::write(&path, "two\n").unwrap();
        ed.feed(":e!<CR>");
        assert_eq!(ed.buf().lines(), ["two"]);
        assert_eq!(ed.doc().stale, None);
    }

    #[test]
    fn an_event_is_ignored_when_the_option_is_turned_off_again() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open(&dir, "one\n");
        watch_on(&mut ed);
        ed.feed(":set nofilewatch<CR>");
        ed.feed(":set autoread<CR>");
        fs::write(&path, "two\n").unwrap();
        ed.handle_event(event_for_disk(&ed, &path));
        assert_eq!(ed.buf().lines(), ["one"]);
        assert_eq!(ed.doc().stale, None);
    }
}

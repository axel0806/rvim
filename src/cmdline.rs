//! Command-line mode: editing keys, `<C-r>` insertion and history. It serves
//! `:` and the `/` and `?` prompts that complete a search motion (`/pat`,
//! `d/pat`, `v?pat`). For a search the keys before the prompt wait in
//! `cmd_prefix`, and `<CR>` replays the whole command with the typed text, so
//! `.` repeats the final pattern rather than the keys that recalled it.

use crate::editor::{Editor, Mode};
use crate::key::Key;
use crate::normal::{Body, Cmd, Motion, Parse, Target, parse};

/// Vim's default 'history'.
const HISTORY_SIZE: usize = 200;

/// What the command line is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PromptKind {
    /// `:`
    Ex,
    /// `/` or `?`, with the prompt character.
    Search(char),
}

/// State of a `<C-r>` waiting for its register name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteState {
    /// After `<C-r>`.
    Reg,
    /// After `<C-r><C-r>`, `<C-r><C-o>` or `<C-r><C-p>`: a register follows.
    Literal,
}

fn chars_len(s: &str) -> usize {
    s.chars().count()
}

fn byte_at(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b)
}

/// Adds `s` as the newest entry, dropping an older copy (Vim's `histadd`).
/// Empty text is not kept. Every search pattern goes here, whether typed at
/// a prompt or used by `*`, `:s`, `:g` or an Ex address (as in Vim).
pub(crate) fn push_history(hist: &mut Vec<String>, s: &str) {
    if s.is_empty() {
        return;
    }
    hist.retain(|h| h != s);
    hist.push(s.to_string());
    if hist.len() > HISTORY_SIZE {
        hist.remove(0);
    }
}

impl Editor {
    /// Enters Command-line mode with `initial` already typed. `ret` is the
    /// mode to come back to when the prompt ends.
    pub(crate) fn cmd_start(&mut self, kind: PromptKind, ret: Mode, initial: &str) {
        self.mode = Mode::CmdLine;
        self.cmd_kind = kind;
        self.cmd_return = ret;
        self.cmdline = initial.to_string();
        self.cmdpos = chars_len(initial);
        self.hist_idx = None;
        self.cmd_paste = None;
        self.message = None;
    }

    /// `/` or `?` that completes a search motion (`/`, `d/`, `2?`) opens the
    /// prompt, keeping the keys before it. Other keys are left alone.
    pub(crate) fn search_prompt_start(&mut self, key: Key) {
        let Key::Char(lead @ ('/' | '?')) = key else {
            return;
        };
        let mut probe = self.pending.clone();
        probe.push(Key::Enter);
        let is_search = matches!(
            parse(&probe, self.is_visual()),
            Parse::Done(Cmd {
                body: Body::Motion(Motion::Search { .. })
                    | Body::Op {
                        target: Target::Motion(Motion::Search { .. }),
                        ..
                    },
                ..
            })
        );
        if !is_search {
            return;
        }
        self.cmd_prefix = std::mem::take(&mut self.pending);
        self.cmd_prefix.pop();
        let ret = self.mode;
        self.cmd_start(PromptKind::Search(lead), ret, "");
    }

    /// Inserts text at the cursor of the command line.
    pub(crate) fn cmd_insert(&mut self, s: &str) {
        self.cmdpos = self.cmdpos.min(chars_len(&self.cmdline));
        let at = byte_at(&self.cmdline, self.cmdpos);
        self.cmdline.insert_str(at, s);
        self.cmdpos += chars_len(s);
    }

    /// Cursor position in the command line, in characters.
    pub fn cmdline_pos(&self) -> usize {
        self.cmdpos.min(chars_len(&self.cmdline))
    }

    /// The prompt character shown before the text: `:`, `/` or `?`.
    pub fn cmdline_prompt(&self) -> char {
        match self.cmd_kind {
            PromptKind::Ex => ':',
            PromptKind::Search(c) => c,
        }
    }

    fn cmd_delete(&mut self, from: usize, to: usize) {
        let (a, b) = (byte_at(&self.cmdline, from), byte_at(&self.cmdline, to));
        self.cmdline.replace_range(a..b, "");
        self.cmdpos = from;
    }

    /// Esc, Ctrl-C, or `<BS>` on an empty line: back to the mode the prompt
    /// was opened from (Visual mode for a search started there).
    fn cmd_cancel(&mut self) {
        self.mode = self.cmd_return;
        self.cmdline.clear();
        self.cmdpos = 0;
        self.hist_idx = None;
        self.cmd_prefix.clear();
    }

    pub(crate) fn cmdline_key(&mut self, key: Key) {
        // Only commands that move the cursor reset the desired column.
        self.keep_curswant = true;
        let len = chars_len(&self.cmdline);
        self.cmdpos = self.cmdpos.min(len);
        if std::mem::take(&mut self.cmd_literal) {
            let c = match key {
                Key::Char(c) => Some(c),
                Key::Esc => Some('\x1b'),
                Key::Enter => Some('\r'),
                Key::Tab => Some('\t'),
                Key::Backspace => Some('\x08'),
                Key::Ctrl(c) => Some(((c as u8) & 0x1f) as char),
                _ => None,
            };
            if let Some(c) = c {
                self.cmd_insert(&c.to_string());
            }
            return;
        }
        if let Some(state) = self.cmd_paste.take() {
            return self.cmd_paste_key(state, key);
        }
        match key {
            Key::Ctrl('v') | Key::Ctrl('q') => self.cmd_literal = true,
            Key::Ctrl('r') => self.cmd_paste = Some(PasteState::Reg),
            Key::Esc | Key::Ctrl('c') => self.cmd_cancel(),
            Key::Enter => {
                let cmd = std::mem::take(&mut self.cmdline);
                self.cmdpos = 0;
                self.hist_idx = None;
                if let PromptKind::Search(lead) = self.cmd_kind {
                    return self.search_finish(lead, &cmd);
                }
                self.mode = Mode::Normal;
                push_history(&mut self.cmd_hist, &cmd);
                self.execute_ex(&cmd);
            }
            Key::Backspace => {
                if self.cmdpos > 0 {
                    self.cmd_delete(self.cmdpos - 1, self.cmdpos);
                } else if len == 0 {
                    self.cmd_cancel();
                }
            }
            Key::Delete => {
                if self.cmdpos < len {
                    self.cmd_delete(self.cmdpos, self.cmdpos + 1);
                } else if self.cmdpos > 0 {
                    self.cmd_delete(self.cmdpos - 1, self.cmdpos);
                }
            }
            Key::Ctrl('u') => self.cmd_delete(0, self.cmdpos),
            Key::Ctrl('w') => {
                let chars: Vec<char> = self.cmdline.chars().collect();
                let mut i = self.cmdpos;
                while i > 0 && matches!(chars[i - 1], ' ' | '\t') {
                    i -= 1;
                }
                if i > 0 {
                    let cls = crate::motion::char_class(chars[i - 1]);
                    while i > 0 && crate::motion::char_class(chars[i - 1]) == cls {
                        i -= 1;
                    }
                }
                self.cmd_delete(i, self.cmdpos);
            }
            Key::Left if self.cmdpos > 0 => self.cmdpos -= 1,
            Key::Right if self.cmdpos < len => self.cmdpos += 1,
            Key::Home | Key::Ctrl('b') => self.cmdpos = 0,
            Key::End | Key::Ctrl('e') => self.cmdpos = len,
            Key::Up => self.history_step(true, true),
            Key::Down => self.history_step(false, true),
            Key::Ctrl('p') => self.history_step(true, false),
            Key::Ctrl('n') => self.history_step(false, false),
            Key::Char(c) => self.cmd_insert(&c.to_string()),
            Key::Tab => self.cmd_insert("\t"),
            _ => {}
        }
    }

    /// The history `<Up>` and friends walk: `:` commands or search patterns.
    fn history(&self) -> &[String] {
        match self.cmd_kind {
            PromptKind::Ex => &self.cmd_hist,
            PromptKind::Search(_) => &self.search_hist,
        }
    }

    /// `<Up>`/`<Down>` (matching what was typed) and `<C-p>`/`<C-n>` (any).
    fn history_step(&mut self, older: bool, filter: bool) {
        if self.hist_idx.is_none() {
            self.hist_prefix = self.cmdline.clone();
        }
        let prefix = if filter {
            self.hist_prefix.clone()
        } else {
            String::new()
        };
        let hist = self.history();
        let n = hist.len();
        let mut i = self.hist_idx.unwrap_or(n);
        let found = loop {
            if older {
                if i == 0 {
                    break None;
                }
                i -= 1;
            } else {
                i += 1;
                if i >= n {
                    break Some(None);
                }
            }
            if hist[i].starts_with(&prefix) {
                break Some(Some(i));
            }
        };
        match found {
            None => self.beep(),
            Some(None) => {
                // Past the newest entry: back to what was typed.
                if self.hist_idx.is_some() {
                    self.cmdline = self.hist_prefix.clone();
                    self.cmdpos = chars_len(&self.cmdline);
                    self.hist_idx = None;
                } else {
                    self.beep();
                }
            }
            Some(Some(i)) => {
                self.hist_idx = Some(i);
                self.cmdline = self.history()[i].clone();
                self.cmdpos = chars_len(&self.cmdline);
            }
        }
    }

    /// `<CR>` on a search prompt: the pattern goes to the search history and
    /// the search motion runs with the typed text, as if its keys had been
    /// typed in one go (the keys before the prompt are restored first).
    fn search_finish(&mut self, lead: char, text: &str) {
        push_history(&mut self.search_hist, text);
        self.mode = self.cmd_return;
        let mut keys = std::mem::take(&mut self.cmd_prefix);
        keys.push(Key::Char(lead));
        keys.extend(text.chars().map(Key::Char));
        keys.push(Key::Enter);
        self.pending = keys;
        // The prompt's own keys set the desired column to keep; the search
        // resets it like any other command.
        self.keep_curswant = false;
        self.run_pending(Key::Enter);
    }

    /// The key after `<C-r>`: a register name or `<C-w>`, `<C-a>`, `<C-l>`.
    fn cmd_paste_key(&mut self, state: PasteState, key: Key) {
        let text = match key {
            Key::Ctrl('r' | 'o' | 'p') if state == PasteState::Reg => {
                self.cmd_paste = Some(PasteState::Literal);
                return;
            }
            Key::Ctrl('w') if state == PasteState::Reg => self.ident_under_cursor().map(|(_, w)| w),
            Key::Ctrl('a') if state == PasteState::Reg => {
                let line: Vec<char> = self.buf.line(self.cursor.line).chars().collect();
                let col = self.cursor.col.min(line.len().saturating_sub(1));
                let blank = |c: char| c == ' ' || c == '\t';
                if line.is_empty() || blank(line[col]) {
                    None
                } else {
                    let mut a = col;
                    while a > 0 && !blank(line[a - 1]) {
                        a -= 1;
                    }
                    let mut b = col;
                    while b < line.len() && !blank(line[b]) {
                        b += 1;
                    }
                    Some(line[a..b].iter().collect())
                }
            }
            Key::Ctrl('l') if state == PasteState::Reg => {
                Some(self.buf.line(self.cursor.line).to_string())
            }
            Key::Char(c) => self.cmd_register_text(c),
            _ => None,
        };
        match text {
            Some(t) => self.cmd_insert(&t),
            None => self.beep(),
        }
    }

    fn cmd_register_text(&self, name: char) -> Option<String> {
        match name {
            '/' => return self.last_pat.clone(),
            ':' => return self.cmd_hist.last().cloned(),
            '%' => return self.buf.path.as_ref().map(|p| p.display().to_string()),
            '"' => return self.cmd_register_text('\0'),
            _ => {}
        }
        let (kind, lines) = if name == '\0' {
            let r = self.regs.get('"')?;
            (r.kind, r.lines.clone())
        } else {
            let r = self.regs.get(name)?;
            (r.kind, r.lines.clone())
        };
        let mut out = lines.join("\r");
        if kind == crate::register::RegType::Line {
            out.push('\r');
        }
        Some(out)
    }
}

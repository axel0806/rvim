//! Command-line mode (`:`): editing keys, `<C-r>` insertion and history.
//! (`/` and `?` text is parsed with the Normal-mode keys, see normal.rs.)

use crate::editor::{Editor, Mode};
use crate::key::Key;

const HISTORY_SIZE: usize = 50;

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

impl Editor {
    /// Enters Command-line mode with `initial` already typed.
    pub(crate) fn cmd_start(&mut self, initial: &str) {
        self.mode = Mode::CmdLine;
        self.cmdline = initial.to_string();
        self.cmdpos = chars_len(initial);
        self.hist_idx = None;
        self.cmd_paste = None;
        self.message = None;
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

    fn cmd_delete(&mut self, from: usize, to: usize) {
        let (a, b) = (byte_at(&self.cmdline, from), byte_at(&self.cmdline, to));
        self.cmdline.replace_range(a..b, "");
        self.cmdpos = from;
    }

    fn cmd_cancel(&mut self) {
        self.mode = Mode::Normal;
        self.cmdline.clear();
        self.cmdpos = 0;
        self.hist_idx = None;
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
                self.mode = Mode::Normal;
                let cmd = std::mem::take(&mut self.cmdline);
                self.cmdpos = 0;
                self.hist_idx = None;
                self.remember_command(&cmd);
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

    fn remember_command(&mut self, cmd: &str) {
        if cmd.is_empty() {
            return;
        }
        self.cmd_hist.retain(|h| h != cmd);
        self.cmd_hist.push(cmd.to_string());
        if self.cmd_hist.len() > HISTORY_SIZE {
            self.cmd_hist.remove(0);
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
        let n = self.cmd_hist.len();
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
            if self.cmd_hist[i].starts_with(&prefix) {
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
                self.cmdline = self.cmd_hist[i].clone();
                self.cmdpos = chars_len(&self.cmdline);
            }
        }
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

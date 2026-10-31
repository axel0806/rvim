//! Macros: `q{reg}` records typed keys, `@{reg}` replays a register.

use crate::editor::Editor;
use crate::key::Key;
use crate::register::{RegType, Register};

/// Private-use characters stand for keys that have no character of their own.
const PUA: char = '\u{E000}';
const SPECIAL: [Key; 11] = [
    Key::Left,
    Key::Right,
    Key::Up,
    Key::Down,
    Key::Home,
    Key::End,
    Key::Delete,
    Key::PageUp,
    Key::PageDown,
    Key::Insert,
    Key::BackTab,
];

/// The text a key leaves in a register while recording.
pub(crate) fn key_to_chars(key: Key, out: &mut String) {
    match key {
        Key::Char(c) => out.push(c),
        Key::Esc => out.push('\x1b'),
        Key::Enter => out.push('\r'),
        Key::Tab => out.push('\t'),
        Key::Backspace => out.push('\x08'),
        Key::Ctrl(c) if c.is_ascii() => out.push(((c as u8) & 0x1f) as char),
        Key::Ctrl(c) => out.push(c),
        Key::F(n) => out.push(char::from_u32(PUA as u32 + 0x20 + u32::from(n)).unwrap_or(PUA)),
        Key::Alt(c) => {
            out.push('\x1b');
            out.push(c);
        }
        k => {
            if let Some(i) = SPECIAL.iter().position(|&s| s == k) {
                out.push(char::from_u32(PUA as u32 + i as u32).unwrap_or(PUA));
            }
        }
    }
}

/// The keys a register's text stands for when executed.
pub(crate) fn chars_to_keys(text: &str) -> Vec<Key> {
    text.chars()
        .map(|c| match c {
            '\x1b' => Key::Esc,
            '\r' => Key::Enter,
            '\t' => Key::Tab,
            '\n' => Key::Ctrl('j'),
            '\x08' | '\x7f' => Key::Backspace,
            c if (c as u32) < 0x20 => Key::Ctrl(((c as u8) | 0x60) as char),
            c if (PUA as u32..PUA as u32 + 0x20).contains(&(c as u32)) => {
                SPECIAL[(c as u32 - PUA as u32) as usize % SPECIAL.len()]
            }
            c if (PUA as u32 + 0x20..PUA as u32 + 0x40).contains(&(c as u32)) => {
                Key::F((c as u32 - PUA as u32 - 0x20) as u8)
            }
            c => Key::Char(c),
        })
        .collect()
}

impl Editor {
    /// `q{reg}`: starts recording typed keys.
    pub(crate) fn start_recording(&mut self, reg: char) {
        if !(reg.is_ascii_alphanumeric() || reg == '"') {
            self.beep();
            return;
        }
        self.recording = Some(reg);
        self.rec_keys.clear();
        self.message = None;
    }

    /// The `q` that ends a recording; the register gets the keys typed since.
    pub(crate) fn stop_recording(&mut self) {
        let Some(reg) = self.recording.take() else {
            return;
        };
        let mut text = String::new();
        for k in std::mem::take(&mut self.rec_keys) {
            key_to_chars(k, &mut text);
        }
        let name = reg.to_ascii_lowercase();
        let old = self.regs.get(name).cloned();
        let new = match old {
            Some(mut old) if reg.is_ascii_uppercase() => {
                if old.kind == RegType::Line {
                    old.lines.push(text);
                } else if let Some(last) = old.lines.last_mut() {
                    last.push_str(&text);
                } else {
                    old.lines.push(text);
                }
                old
            }
            _ => Register {
                kind: RegType::Char,
                lines: vec![text],
            },
        };
        // Recording does not change what `""` points to.
        self.regs.set(Some(name), Some(new));
    }

    /// `[count]@{reg}`, `@@` and `@:`. An error ends the whole execution.
    pub(crate) fn exec_register(&mut self, reg: char, count: usize) {
        let reg = if reg == '@' {
            match self.last_macro {
                Some(r) => r,
                None => {
                    self.error("E748: No previously used register");
                    return;
                }
            }
        } else {
            reg
        };
        if reg == ':' {
            self.last_macro = Some(':');
            let Some(cmd) = self.cmd_hist.last().cloned() else {
                self.error("E30: No previous command line");
                return;
            };
            for _ in 0..count {
                self.execute_ex(&cmd);
            }
            return;
        }
        let Some(r) = self.regs.get(reg).cloned() else {
            self.beep();
            return;
        };
        self.last_macro = Some(reg);
        let mut text = r.lines.join("\n");
        if r.kind == RegType::Line {
            text.push('\n');
        }
        let keys = chars_to_keys(&text);
        let start = self.beeps;
        self.replaying += 1;
        'outer: for _ in 0..count {
            for &k in &keys {
                self.handle_key(k);
                if self.beeps != start || self.quit {
                    break 'outer;
                }
            }
        }
        self.replaying -= 1;
    }
}

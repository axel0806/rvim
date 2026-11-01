// SPDX-License-Identifier: Vim
// Derived in part from Vim (https://www.vim.org); see LICENSE-VIM.
// This is a modified version, not the official Vim.
//! Registers, following Vim's register.c: `"0` gets yanks, `"1`-`"9` shift
//! on multi-line deletes, `"-` gets small deletes, `"a`-`"z` (append with
//! `"A`-`"Z`), `"_` discards, and `""` points at the last written register.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegType {
    Char,
    Line,
    /// Blockwise, with the block width minus one (Vim's `y_width`).
    Block(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Register {
    pub kind: RegType,
    pub lines: Vec<String>,
}

const DELETION: usize = 36;
const COUNT: usize = 37;

pub struct Registers {
    slots: Vec<Option<Register>>,
    /// Register being written or read (Vim's `y_current`).
    current: usize,
    /// Register `""` refers to (Vim's `y_previous`).
    previous: Option<usize>,
    /// Writing to an uppercase register appends (Vim's `y_append`).
    append: bool,
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

/// A register name that can be written (`"x` before a command).
pub fn valid_name(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '"' | '-' | '_')
}

impl Registers {
    pub fn new() -> Self {
        Self {
            slots: vec![None; COUNT],
            current: 0,
            previous: None,
            append: false,
        }
    }

    /// Vim's `get_yank_register()`. `name` is `None` for no register.
    pub fn select(&mut self, name: Option<char>, writing: bool) {
        self.append = false;
        if matches!(name, None | Some('"'))
            && !writing
            && let Some(p) = self.previous
        {
            self.current = p;
            return;
        }
        let i = match name {
            Some(c @ '0'..='9') => c as usize - '0' as usize,
            Some(c @ 'a'..='z') => c as usize - 'a' as usize + 10,
            Some(c @ 'A'..='Z') => {
                self.append = true;
                c as usize - 'A' as usize + 10
            }
            Some('-') => DELETION,
            _ => 0,
        };
        self.current = i;
        if writing {
            self.previous = Some(i);
        }
    }

    /// Vim's `reset_y_append()`.
    pub fn reset_append(&mut self) {
        self.append = false;
    }

    /// Stores `reg` in the current register, appending for `"A`-`"Z`
    /// (the tail of Vim's `op_yank()`).
    pub fn store(&mut self, reg: Register) {
        let slot = &mut self.slots[self.current];
        match slot {
            Some(old) if self.append => {
                if reg.kind == RegType::Line {
                    old.kind = RegType::Line;
                }
                let mut lines = reg.lines.into_iter();
                if old.kind == RegType::Char
                    && let Some(first) = lines.next()
                {
                    old.lines.last_mut().unwrap().push_str(&first);
                }
                old.lines.extend(lines);
            }
            _ => *slot = Some(reg),
        }
    }

    /// Vim's `shift_delete_registers()`: `"1` moves to `"2` and so on, and
    /// `"1` becomes current.
    pub fn shift_delete(&mut self) {
        for n in (2..=9).rev() {
            self.slots[n] = self.slots[n - 1].take();
        }
        self.current = 1;
        if !self.append {
            self.previous = Some(1);
        }
        self.slots[1] = None;
    }

    /// The current register's contents (after `select(.., false)`).
    pub fn current(&self) -> Option<&Register> {
        self.slots[self.current].as_ref()
    }

    /// Contents of the named register as `getreg()` sees it.
    pub fn get(&self, name: char) -> Option<&Register> {
        let i = match name {
            '"' => self.previous?,
            '0'..='9' => name as usize - '0' as usize,
            'a'..='z' => name as usize - 'a' as usize + 10,
            'A'..='Z' => name as usize - 'A' as usize + 10,
            '-' => DELETION,
            _ => return None,
        };
        self.slots[i].as_ref()
    }

    /// Replaces a register outright (used by Visual `p` to restore one).
    pub fn set(&mut self, name: Option<char>, reg: Option<Register>) {
        let i = match name {
            None | Some('"') => match self.previous {
                Some(p) => p,
                None => return,
            },
            Some(c @ '0'..='9') => c as usize - '0' as usize,
            Some(c @ ('a'..='z' | 'A'..='Z')) => {
                c.to_ascii_lowercase() as usize - 'a' as usize + 10
            }
            Some('-') => DELETION,
            _ => return,
        };
        self.slots[i] = reg;
    }
}

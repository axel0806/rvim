// SPDX-License-Identifier: Apache-2.0
//! The start menu (PLAN.md, "Start menu"): a read-only overlay drawn on the
//! Frame when rvim starts with no file and an empty buffer, and the option
//! `startmenu` is on (`--start-menu` turns it on; there is no config file
//! yet). It is not buffer text: the buffer, cursor and registers are the
//! same as with no menu. While it is shown only `<Up>`, `<Down>`, `<CR>` and
//! `<Esc>` are taken; any other key closes it and is handled as usual.

use crate::VERSION_MESSAGE;
use crate::editor::Editor;
use crate::key::Key;
use crate::options::Options;
use crate::view::Hl;

/// The logo is hidden below these sizes (PLAN.md, "Content").
const LOGO_MIN_COLS: usize = 60;
const LOGO_MIN_ROWS: usize = 20;

/// The logo until a designed one exists (PLAN.md, "Logo design").
const LOGO: [&str; 3] = ["+------+", "| rvim |", "+------+"];

const HINT: &str = "arrows select  Enter open  Esc close";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// An empty unnamed buffer. The menu only shows at startup, when the
    /// buffer already is one, so there is nothing to do (`:enew` is not
    /// implemented yet).
    NewFile,
    /// Puts `:e ` on the command line.
    OpenFile,
    /// `:help`. Shown dimmed until `:help` exists.
    Help,
    /// `:q`.
    Quit,
}

pub(crate) struct Item {
    pub label: &'static str,
    pub action: Action,
    pub enabled: bool,
}

/// The entries in display order. Recent files and Config wait for the
/// state directory and the config file; the agent line waits for milestone 7.
pub(crate) const ITEMS: &[Item] = &[
    Item {
        label: "New file",
        action: Action::NewFile,
        enabled: true,
    },
    Item {
        label: "Open file",
        action: Action::OpenFile,
        enabled: true,
    },
    Item {
        label: "Help",
        action: Action::Help,
        enabled: false,
    },
    Item {
        label: "Quit",
        action: Action::Quit,
        enabled: true,
    },
];

#[derive(Clone, Debug)]
pub(crate) struct StartMenu {
    /// Index into `ITEMS`; always an enabled item.
    selected: usize,
}

/// What a key does to the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The key moved the selection; the menu stays open.
    Stay,
    /// `<Esc>`: the menu closes and nothing else happens.
    Close,
    /// `<CR>`: the menu closes and the selected entry runs.
    Run(Action),
    /// Any other key: the menu closes and the key is handled as usual.
    Pass,
}

impl StartMenu {
    fn new() -> Self {
        StartMenu {
            selected: ITEMS.iter().position(|i| i.enabled).unwrap_or(0),
        }
    }

    pub(crate) fn key(&mut self, key: Key) -> Outcome {
        match key {
            Key::Up => {
                self.step(-1);
                Outcome::Stay
            }
            Key::Down => {
                self.step(1);
                Outcome::Stay
            }
            Key::Enter => Outcome::Run(ITEMS[self.selected].action),
            Key::Esc => Outcome::Close,
            _ => Outcome::Pass,
        }
    }

    /// Moves to the next enabled entry in direction `dir`; stops at the ends.
    fn step(&mut self, dir: isize) {
        let mut i = self.selected as isize;
        loop {
            i += dir;
            if i < 0 || i >= ITEMS.len() as isize {
                return;
            }
            if ITEMS[i as usize].enabled {
                self.selected = i as usize;
                return;
            }
        }
    }
}

/// The menu's rows, highlight groups and cursor for a window of `cols` x
/// `screen_rows` (the last row is the status line, drawn by the caller).
pub(crate) struct MenuFrame {
    pub rows: Vec<String>,
    pub hl: Vec<Vec<Hl>>,
    pub cursor: (usize, usize),
}

/// Lays the menu out, centred in the window. The version line is always
/// shown (the Vim license requires the modified-version notice).
pub(crate) fn layout(menu: &StartMenu, cols: usize, screen_rows: usize) -> MenuFrame {
    let cols = cols.max(1);
    let text_h = screen_rows.saturating_sub(1).max(1);
    // Each line: its text and the highlight group of its whole row.
    let mut lines: Vec<(String, Hl)> = Vec::new();
    let mut cursor_line = 0;
    let mut cursor_col = 0;
    if cols >= LOGO_MIN_COLS && screen_rows >= LOGO_MIN_ROWS {
        for l in LOGO {
            lines.push((centred(l, cols), Hl::Normal));
        }
        lines.push((String::new(), Hl::Normal));
    }
    for l in wrap(VERSION_MESSAGE, cols.saturating_sub(2).max(1)) {
        lines.push((centred(&l, cols), Hl::Normal));
    }
    lines.push((String::new(), Hl::Normal));
    let block_w = ITEMS.iter().map(|i| i.label.len()).max().unwrap_or(0);
    let left = cols.saturating_sub(block_w) / 2;
    for (i, item) in ITEMS.iter().enumerate() {
        let hl = if i == menu.selected {
            cursor_line = lines.len();
            cursor_col = left;
            Hl::MenuSelected
        } else if item.enabled {
            Hl::Normal
        } else {
            Hl::Dimmed
        };
        let text = format!("{}{:<w$}", " ".repeat(left), item.label, w = block_w);
        lines.push((text, hl));
    }
    lines.push((String::new(), Hl::Normal));
    lines.push((centred(HINT, cols), Hl::Normal));

    // Vertical centring; a menu taller than the window is cut at the bottom.
    let top = text_h.saturating_sub(lines.len()) / 2;
    let mut rows = Vec::with_capacity(text_h);
    let mut hl = Vec::with_capacity(text_h);
    for r in 0..text_h {
        let line = r.checked_sub(top).and_then(|i| lines.get(i));
        let (text, group) = match line {
            Some((t, g)) => (t.as_str(), *g),
            None => ("", Hl::Normal),
        };
        let mut row: String = text.chars().take(cols).collect();
        let used = row.chars().count();
        row.extend(std::iter::repeat_n(' ', cols - used));
        rows.push(row);
        hl.push(vec![group; cols]);
    }
    MenuFrame {
        rows,
        hl,
        cursor: (cursor_col.min(cols - 1), cursor_line + top),
    }
}

/// `text` centred in `cols` cells (cut to `cols`).
fn centred(text: &str, cols: usize) -> String {
    let len = text.chars().count();
    if len >= cols {
        return text.chars().take(cols).collect();
    }
    format!("{}{}", " ".repeat((cols - len) / 2), text)
}

/// Word wrap at spaces. A word longer than `width` stays on its own line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        let len = cur.chars().count() + word.chars().count();
        if !cur.is_empty() && len + 1 > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl Editor {
    /// Turns the start menu on for this run (`--start-menu`): sets the
    /// option `startmenu`.
    pub fn enable_start_menu(&mut self) {
        if let Some(i) = Options::find("startmenu") {
            self.opts.set(i, crate::options::Val::Bool(true));
        }
    }

    /// Shows the start menu when the option is on, no file was given and the
    /// buffer is empty and unmodified. Returns whether it is shown; when it
    /// is not, the caller shows the intro message as before.
    pub fn show_start_menu(&mut self) -> bool {
        let empty = self.buf.line_count() == 1 && self.buf.line_len(0) == 0;
        if !self.opts.startmenu() || self.buf.path.is_some() || self.buf.modified || !empty {
            return false;
        }
        self.start_menu = Some(StartMenu::new());
        true
    }

    pub fn start_menu_shown(&self) -> bool {
        self.start_menu.is_some()
    }

    /// Closes the menu without running anything (a paste closes it: pasted
    /// text is data, and the menu takes only the keys listed in the plan).
    pub(crate) fn close_start_menu(&mut self) {
        self.start_menu = None;
    }

    /// Handles `key` while the menu is shown. Returns false when the key is
    /// not for the menu: the menu is then closed and the key is handled as
    /// usual by the caller.
    pub(crate) fn start_menu_key(&mut self, key: Key) -> bool {
        let outcome = match self.start_menu.as_mut() {
            Some(menu) => menu.key(key),
            None => return false,
        };
        match outcome {
            Outcome::Stay => true,
            Outcome::Close => {
                self.start_menu = None;
                true
            }
            Outcome::Pass => {
                self.start_menu = None;
                false
            }
            Outcome::Run(action) => {
                self.start_menu = None;
                self.run_start_action(action);
                true
            }
        }
    }

    /// Runs an entry as if the user typed its Ex command.
    fn run_start_action(&mut self, action: Action) {
        match action {
            Action::NewFile | Action::Help => {}
            Action::OpenFile => self.feed(":e "),
            Action::Quit => self.feed(":q<CR>"),
        }
    }
}

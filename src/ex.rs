// SPDX-License-Identifier: Vim
// Derived in part from Vim (https://www.vim.org); see LICENSE-VIM.
// This is a modified version, not the official Vim.
//! Ex commands typed after `:`: line ranges (numbers, `.`, `$`, `%`, `'<`,
//! `/pat/`, offsets, `,` and `;`), files (`:w :q :wq :x :e :r`), line commands
//! (`:d :y :pu :m :t :j :> :<`), `:s`, `:g`, `:normal`, `:undo`, `:set`.
//! `:r` reads a file's lines (`:r !cmd` is not supported).

use crate::buffer::{Buffer, PosEdit, read_file};
use crate::editor::{Editor, Message, Mode, describe};
use crate::key::Key;
use crate::motion::{Pos, first_non_blank};
use crate::normal::Op;
use crate::ops::OpArg;
use crate::options::Target;
use crate::regex::{Match, Regex};
use crate::reload::DiskState;
use crate::search::{SearchFlags, Start, searchit};
use std::fs;
use std::path::{Path, PathBuf};

struct Parsed<'a> {
    name: &'a str,
    bang: bool,
    arg: &'a str,
}

/// True if `typed` is an abbreviation of `full` at least `min` chars long.
fn abbrev(typed: &str, full: &str, min: usize) -> bool {
    typed.len() >= min && full.starts_with(typed)
}

/// Line range of a command (1-based lines, as typed).
#[derive(Clone, Copy, Debug)]
struct Range {
    line1: usize,
    line2: usize,
    /// Number of addresses typed.
    count: usize,
}

type ExRes<T> = Result<T, String>;

/// The first position of `s` that is not part of an Ex address.
fn skip_ws(s: &str) -> &str {
    s.trim_start_matches([' ', '\t'])
}

/// Splits at the first unescaped `delim`, skipping `[]` collections like
/// `skip_regexp()`. Returns the text before, and the rest after the
/// delimiter (if there was one).
fn split_at_delim(s: &str, delim: char) -> (String, Option<&str>) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c == delim {
            let rest_start: usize = chars[..=i].iter().map(|c| c.len_utf8()).sum();
            return (out, Some(&s[rest_start..]));
        }
        if c == '\\' && i + 1 < chars.len() {
            // `\/` stands for a literal delimiter.
            if chars[i + 1] == delim && delim != '\\' {
                out.push(delim);
            } else {
                out.push(c);
                out.push(chars[i + 1]);
            }
            i += 2;
            continue;
        }
        if c == '[' {
            // A collection: the delimiter does not end the pattern in it.
            let mut j = i + 1;
            if chars.get(j) == Some(&'^') {
                j += 1;
            }
            if chars.get(j) == Some(&']') {
                j += 1;
            }
            while j < chars.len() && chars[j] != ']' {
                if chars[j] == '\\' && j + 1 < chars.len() {
                    j += 1;
                }
                j += 1;
            }
            if j < chars.len() {
                out.extend(&chars[i..=j]);
                i = j + 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    (out, None)
}

/// One piece of work of an Ex command line, run from a stack (Vim's
/// `do_cmdline()` loop and the `:g` loop). A stack, not a loop, so that a
/// `c` flag prompt of `:s` can stop in the middle and go on after the answer.
pub(crate) enum ExTask {
    /// A command line, which may hold several `|`-separated commands.
    Ex(String),
    /// `:g`: run `cmd` on the marked lines from index `next` of `buf.tracked`.
    GlobalLines { cmd: String, next: usize },
    /// `:g` is over: the cursor and the report of changed lines.
    GlobalEnd { old_lines: i64 },
}

impl Editor {
    pub(crate) fn execute_ex(&mut self, cmd: &str) {
        let base = self.ex_tasks.len();
        self.ex_tasks.push(ExTask::Ex(cmd.to_string()));
        self.run_ex_tasks(base);
    }

    /// Runs queued tasks above `base` until none is left or a prompt waits
    /// for an answer (then `sub` holds the `:s` and the tasks wait).
    pub(crate) fn run_ex_tasks(&mut self, base: usize) {
        while self.ex_tasks.len() > base && self.mode != Mode::Confirm {
            let Some(task) = self.ex_tasks.pop() else {
                break;
            };
            match task {
                ExTask::Ex(cmd) => {
                    if self.quit {
                        continue;
                    }
                    // `|` separates commands, except after :g and :normal.
                    let (this, next) = self.split_bar(&cmd);
                    // An error is reported, and the rest of the line still
                    // runs (as in Vim, where only `:try` stops it).
                    if let Err(msg) = self.execute_one(&this) {
                        self.error(msg);
                    }
                    match (next, self.sub.as_mut()) {
                        // A `:s` is waiting for an answer: the rest of the
                        // line runs when it has finished.
                        (Some(rest), Some(sub)) => sub.rest = Some(rest),
                        (Some(rest), None) => self.ex_tasks.push(ExTask::Ex(rest)),
                        (None, _) => {}
                    }
                }
                ExTask::GlobalLines { cmd, next } => {
                    if self.quit || next >= self.buf().tracked.len() {
                        continue;
                    }
                    self.ex_tasks.push(ExTask::GlobalLines {
                        cmd: cmd.clone(),
                        next: next + 1,
                    });
                    // Lines deleted by an earlier command are marked usize::MAX.
                    let l = self.buf().tracked[next];
                    if l != usize::MAX {
                        self.win_mut().cursor = Pos::new(l.min(self.buf_mut().last_line()), 0);
                        self.ex_tasks.push(ExTask::Ex(cmd));
                    }
                }
                ExTask::GlobalEnd { old_lines } => {
                    self.global_busy = false;
                    self.buf_mut().tracked.clear();
                    if self.global_need_beginline {
                        self.win_mut().cursor.col =
                            first_non_blank(self.buf(), self.win().cursor.line, true);
                    } else {
                        let len = self.buf().line_len(self.win().cursor.line);
                        if self.win().cursor.col >= len {
                            self.win_mut().cursor.col = len.saturating_sub(1);
                        }
                    }
                    let diff = self.buf().line_count() as i64 - old_lines;
                    if diff.abs() > 2 {
                        let what = if diff > 0 {
                            "more lines"
                        } else {
                            "fewer lines"
                        };
                        self.info(format!("{} {what}", diff.abs()));
                    }
                }
            }
        }
    }

    /// Splits `cmd` at the first `|` that separates commands.
    fn split_bar(&self, cmd: &str) -> (String, Option<String>) {
        let t = cmd.trim_start_matches([' ', '\t', ':']);
        // Skip the range to find the command name.
        let lead = cmd.len() - t.len();
        // A crude skip over addresses: find first alphabetic char not in
        // a pattern. Good enough to recognize :g and :normal.
        let mut depth = false;
        let mut end = 0;
        for (i, c) in t.char_indices() {
            if c == '/' || c == '?' {
                depth = !depth;
            }
            if !depth && c.is_ascii_alphabetic() {
                end = i;
                break;
            }
        }
        let name: String = t[end..]
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        let name_end = lead + end + name.len();
        // `:Agent` takes the rest of the line as the prompt, `|` included.
        let takes_bar = abbrev(&name, "global", 1)
            || abbrev(&name, "vglobal", 1)
            || abbrev(&name, "normal", 4)
            || name == "Agent";
        if takes_bar {
            return (cmd.to_string(), None);
        }
        // `:s`: a `|` in the pattern or the replacement (also `\=expr`) is not
        // a separator. Only what follows the replacement's delimiter and the
        // flags is scanned, as Vim's ex_substitute() does with check_nextcmd().
        let from = if abbrev(&name, "substitute", 1) {
            sub_args_end(cmd, name_end).unwrap_or(0)
        } else {
            0
        };
        // In the range (before the name), a `|` inside a `/pat/` or `?pat?`
        // address is not a separator either.
        // The name found by the crude scan above may lie past a `|` that
        // ends a range without a command (`:2|s/x/y/`), so the scan starts
        // at the beginning. Only the pattern and replacement of `:s` (from
        // `name_end` to `from`) are skipped as a whole.
        let range_chars = cmd[..name_end].chars().count();
        let args_end_chars = cmd[..from].chars().count();
        let chars: Vec<char> = cmd.chars().collect();
        let mut i = 0;
        let mut in_addr = false;
        while i < chars.len() {
            match chars[i] {
                // An escaped character (`\|`, `\\`) is never a separator.
                '\\' => {
                    i += 2;
                    continue;
                }
                '/' | '?' if i < range_chars => in_addr = !in_addr,
                '|' if !(i >= range_chars && i < args_end_chars)
                    && (i >= range_chars || !in_addr) =>
                {
                    let a: String = chars[..i].iter().collect();
                    let b: String = chars[i + 1..].iter().collect();
                    return (a, Some(b));
                }
                _ => {}
            }
            i += 1;
        }
        (cmd.to_string(), None)
    }

    fn execute_one(&mut self, cmd: &str) -> ExRes<()> {
        let cmd = cmd.trim_start_matches([' ', '\t', ':']);
        if cmd.is_empty() {
            return Ok(());
        }
        let (range, rest) = self.parse_range(cmd)?;
        let rest = skip_ws(rest);
        if rest.is_empty() {
            if range.count > 0 {
                // `:N` is a jump.
                self.set_pcmark();
                let l = range.line2.clamp(1, self.buf().line_count());
                self.goto_line_first_non_blank(l - 1);
            }
            return Ok(());
        }
        // Command name: letters, or one special character.
        let first = rest.chars().next().unwrap();
        let (name, after) = if first.is_ascii_alphabetic() {
            let n = rest
                .find(|c: char| !c.is_ascii_alphabetic())
                .unwrap_or(rest.len());
            // `:s` and friends directly followed by a delimiter, `:d` with a
            // register, etc. are handled by their own argument parsing.
            let (mut name, mut after) = rest.split_at(n);
            // `:dl`, `:k` forms are not supported; `:g` and `:s` may be
            // followed by a delimiter that is a letter-free char, handled by
            // the split above.
            if let Some(stripped) = name.strip_prefix("normal")
                && stripped.is_empty()
            {
                name = "normal";
            }
            if name.len() > 1 && !is_known_long(name) {
                // e.g. `:sx` or `:dx`: split a single-letter command from its
                // argument (`:d3`, `:ya`).
                let c0 = &name[..1];
                if matches!(c0, "s" | "d" | "y" | "m" | "t" | "j" | "k" | "g" | "v")
                    && !is_known_prefix(name)
                {
                    after = &rest[1..];
                    name = c0;
                }
            }
            (name, after)
        } else if matches!(first, '&' | '<' | '>' | '=' | '#' | '~' | '!' | '@' | '*') {
            rest.split_at(first.len_utf8())
        } else {
            return Err(format!("E492: Not an editor command: {cmd}"));
        };
        let (bang, after) = match after.strip_prefix('!') {
            Some(a) if name != "!" => (true, a),
            _ => (false, after),
        };
        let p = Parsed {
            name,
            bang,
            arg: if name == "normal" {
                after.strip_prefix(' ').unwrap_or(after)
            } else {
                after.trim_start_matches([' ', '\t'])
            },
        };
        let arg_trim = p.arg.trim_end();
        let n = p.name;
        if let Some(msg) = Self::log_refusal(n).filter(|_| self.doc().nomodifiable) {
            return Err(msg.into());
        }
        // Commands that do not accept line 0 use line 1 instead.
        let mut range = range;
        if !(abbrev(n, "put", 2) || abbrev(n, "read", 1)) {
            if range.line1 == 0 {
                range.line1 = 1;
            }
            if range.line2 == 0 {
                range.line2 = 1;
            }
        }
        if abbrev(n, "write", 1) {
            self.cmd_write(&p, false);
        } else if n == "wq" {
            self.cmd_write(&p, true);
        } else if abbrev(n, "xit", 1) || abbrev(n, "exit", 3) {
            if self.buf().modified || !p.arg.is_empty() {
                self.cmd_write(&p, true);
            } else {
                self.quit = true;
            }
        } else if abbrev(n, "quit", 1) || abbrev(n, "qall", 2) {
            if self.buf().modified && !p.bang {
                return Err("E37: No write since last change (add ! to override)".into());
            }
            self.quit = true;
        } else if n == "AgentScan" {
            // rvim only (capital letter): scans for agents on a worker. It
            // connects to nothing (agent_scan.rs).
            if !p.arg.is_empty() {
                return Err(format!("E488: Trailing characters: {}", p.arg));
            }
            self.agent_scan_command();
        } else if n == "AgentConnect" {
            // rvim only: connects to one agent, after the consent prompt for
            // this project (agent_session.rs).
            let name = p.arg.trim();
            self.agent_connect((!name.is_empty()).then_some(name));
        } else if n == "AgentDisconnect" {
            self.agent_disconnect();
        } else if n == "AgentCancel" {
            self.agent_cancel();
        } else if n == "Agent" {
            // rvim only: one prompt turn to the connected agent (agent_turn.rs).
            // The rest of the line is the prompt, as typed.
            self.agent_command(p.arg);
        } else if n == "AgentLog" {
            // rvim only: `:AgentLog` shows the agent log in the window,
            // `:AgentLog!` goes back to the document it replaced.
            if p.bang {
                self.agent_log_back();
            } else {
                self.agent_log_show();
            }
        } else if n == "ConfigWarnings" {
            // rvim only (capital letter, as CLAUDE.md asks): the problems
            // the config file had at startup.
            self.show_config_warnings();
        } else if abbrev(n, "version", 2) {
            // The short notice: the message line cannot show more than one
            // line. The full text is `--version` and the start menu.
            self.info(crate::NOTICE_SHORT);
        } else if abbrev(n, "edit", 1) {
            self.cmd_edit(&p);
        } else if abbrev(n, "checktime", 6) {
            self.ex_checktime(p.arg)?;
        } else if abbrev(n, "read", 1) {
            self.ex_read(range, p.bang, p.arg)?;
        } else if abbrev(n, "delete", 1) {
            self.ex_delete_yank(range, p.arg, true)?;
        } else if abbrev(n, "yank", 1) {
            self.ex_delete_yank(range, p.arg, false)?;
        } else if abbrev(n, "put", 2) {
            self.ex_put(range, p.bang, p.arg)?;
        } else if abbrev(n, "move", 1) {
            self.ex_copymove(range, p.arg, true)?;
        } else if abbrev(n, "copy", 2) || n == "t" {
            self.ex_copymove(range, p.arg, false)?;
        } else if n == "k" || abbrev(n, "mark", 2) {
            self.ex_mark(range, p.arg)?;
        } else if abbrev(n, "jumps", 2) {
            self.ex_jumps();
        } else if n == "marks" {
            self.ex_marks(p.bang, p.arg)?;
        } else if abbrev(n, "delmarks", 4) {
            self.ex_delmarks(p.bang, p.arg)?;
        } else if abbrev(n, "join", 1) {
            self.ex_join(range, p.bang, p.arg)?;
        } else if n == ">" || n == "<" {
            self.ex_shift(range, n == "<", p.arg)?;
        } else if abbrev(n, "substitute", 1) {
            self.ex_substitute(range, p.arg, false)?;
        } else if n == "&" || n == "~" {
            self.ex_substitute(range, p.arg, true)?;
        } else if abbrev(n, "global", 1) {
            self.ex_global(range, p.bang, p.arg)?;
        } else if abbrev(n, "vglobal", 1) {
            self.ex_global(range, true, p.arg)?;
        } else if abbrev(n, "normal", 4) {
            self.ex_normal(range, p.arg);
        } else if abbrev(n, "nohlsearch", 3) {
            self.no_hlsearch = true;
        } else if abbrev(n, "undo", 1) {
            self.undo_cmd(1, false);
        } else if abbrev(n, "redo", 3) {
            self.undo_cmd(1, true);
        } else if abbrev(n, "print", 1)
            || abbrev(n, "number", 2)
            || n == "#"
            || abbrev(n, "list", 1)
        {
            let l = range.line2.clamp(1, self.buf().line_count());
            self.goto_line_first_non_blank(l - 1);
            let text = self.buf().line_string(l - 1);
            self.info(text);
        } else if n == "=" {
            let l = if range.count == 0 {
                self.buf().line_count()
            } else {
                range.line2
            };
            self.info(l.to_string());
        } else if abbrev(n, "set", 2) {
            self.ex_set(arg_trim, Target::Both)?;
        } else if abbrev(n, "setlocal", 4) {
            self.ex_set(arg_trim, Target::Local)?;
        } else if abbrev(n, "setglobal", 4) {
            self.ex_set(arg_trim, Target::Global)?;
        } else if abbrev(n, "silent", 3) {
            return self.execute_one(p.arg);
        } else {
            return Err(format!("E492: Not an editor command: {cmd}"));
        }
        Ok(())
    }

    // ---- ranges ---------------------------------------------------------

    fn parse_range<'a>(&mut self, s: &'a str) -> ExRes<(Range, &'a str)> {
        let mut rest = skip_ws(s);
        let cur = self.win().cursor.line + 1;
        let last = self.buf().line_count();
        let mut base = cur;
        let mut lines: Vec<usize> = Vec::new();
        let mut count = 0usize;
        loop {
            rest = skip_ws(rest);
            if let Some(r) = rest.strip_prefix('%') {
                rest = r;
                lines = vec![1, last];
                count = 2;
            } else if let Some(r) = rest.strip_prefix('*') {
                rest = r;
                let (a, b) = self.visual_lines()?;
                lines = vec![a, b];
                count = 2;
            } else {
                let (addr, r) = self.get_address(rest, base)?;
                rest = r;
                match addr {
                    Some(a) => {
                        lines.push(a);
                        count += 1;
                    }
                    None => {
                        if rest.starts_with([',', ';']) {
                            lines.push(base);
                            count += 1;
                        }
                    }
                }
            }
            rest = skip_ws(rest);
            match rest.chars().next() {
                Some(',') => {
                    rest = &rest[1..];
                    if !rest
                        .trim_start()
                        .starts_with(|c: char| c.is_ascii_digit() || ".$%'/?\\+-".contains(c))
                    {
                        lines.push(cur);
                        count += 1;
                        break;
                    }
                }
                Some(';') => {
                    rest = &rest[1..];
                    if let Some(&l) = lines.last() {
                        if l > last {
                            return Err("E16: Invalid range".into());
                        }
                        base = l.max(1);
                        self.win_mut().cursor.line = base - 1;
                    }
                }
                _ => break,
            }
        }
        let n = lines.len();
        let (mut l1, mut l2) = match n {
            0 => (cur, cur),
            1 => (lines[0], lines[0]),
            _ => (lines[n - 2], lines[n - 1]),
        };
        if count == 0 {
            l1 = cur;
            l2 = cur;
        }
        if l1 > l2 {
            std::mem::swap(&mut l1, &mut l2);
        }
        Ok((
            Range {
                line1: l1,
                line2: l2,
                count,
            },
            rest,
        ))
    }

    fn visual_lines(&self) -> ExRes<(usize, usize)> {
        match self.doc().last_visual {
            Some(v) => {
                let (a, b) = (v.start.line.min(v.end.line), v.start.line.max(v.end.line));
                Ok((a + 1, b + 1))
            }
            None => Err("E20: Mark not set".into()),
        }
    }

    /// One address with its `+N`/`-N` offsets. `base` is the line a
    /// relative address or pattern starts from.
    fn get_address<'a>(&mut self, s: &'a str, base: usize) -> ExRes<(Option<usize>, &'a str)> {
        let mut rest = skip_ws(s);
        let last = self.buf().line_count();
        let mut line: Option<i64> = None;
        match rest.chars().next() {
            Some('.') => {
                rest = &rest[1..];
                line = Some(base as i64);
            }
            Some('$') => {
                rest = &rest[1..];
                line = Some(last as i64);
            }
            Some('\'') => {
                let m = rest[1..].chars().next();
                match m {
                    Some('<') | Some('>') => {
                        let (a, b) = self.visual_lines_exact(m == Some('<'))?;
                        line = Some(if m == Some('<') { a } else { b } as i64);
                        rest = &rest[2..];
                    }
                    Some(c @ ('a'..='z' | 'A'..='Z')) => {
                        let p = self.buf().mark(c).ok_or("E20: Mark not set")?;
                        line = Some(p.line as i64 + 1);
                        rest = &rest[2..];
                    }
                    Some(c @ ('[' | ']' | '"')) => {
                        let p = self.special_mark(c).ok_or("E20: Mark not set")?;
                        line = Some(p.line as i64 + 1);
                        rest = &rest[2..];
                    }
                    Some(_) => return Err("E20: Mark not set".into()),
                    None => return Err("E20: Mark not set".into()),
                }
            }
            Some(c @ ('/' | '?')) => {
                let (pat, after) = split_at_delim(&rest[1..], c);
                rest = after.unwrap_or("");
                let mut found = self.addr_search(&pat, c == '/', base)? as i64;
                // A line offset right after the pattern is part of the
                // search (do_search) and is limited to the buffer.
                if let Some(sign @ ('+' | '-')) =
                    rest.chars().next().filter(|c| matches!(c, '+' | '-'))
                {
                    let body = &rest[1..];
                    let n = body
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(body.len());
                    let v: i64 = if n == 0 {
                        1
                    } else {
                        body[..n].parse().unwrap_or(i64::MAX / 4)
                    };
                    found = (if sign == '+' { found + v } else { found - v }).clamp(1, last as i64);
                    rest = &body[n..];
                } else if rest.starts_with(|c: char| c.is_ascii_digit()) {
                    let n = rest
                        .find(|c: char| !c.is_ascii_digit())
                        .unwrap_or(rest.len());
                    let v: i64 = rest[..n].parse().unwrap_or(i64::MAX / 4);
                    found = (found + v).clamp(1, last as i64);
                    rest = &rest[n..];
                }
                line = Some(found);
                // `/pat1//pat2/`: more patterns continue from the found line.
                while let Some(c2 @ ('/' | '?')) = rest.chars().next() {
                    let (pat, after) = split_at_delim(&rest[1..], c2);
                    rest = after.unwrap_or("");
                    line = Some(self.addr_search(&pat, c2 == '/', line.unwrap() as usize)? as i64);
                }
            }
            Some('\\') => {
                let c = rest[1..].chars().next();
                match c {
                    Some('/') | Some('?') | Some('&') => {
                        let pat = match self.last_pat.clone() {
                            Some(p) => p,
                            None => return Err("E35: No previous regular expression".into()),
                        };
                        line = Some(self.addr_search(&pat, c != Some('?'), base)? as i64);
                        rest = &rest[2..];
                    }
                    _ => return Err("E10: \\ should be followed by /, ? or &".into()),
                }
            }
            Some(d) if d.is_ascii_digit() => {
                let n = rest
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(rest.len());
                line = Some(rest[..n].parse().unwrap_or(i64::MAX / 4));
                rest = &rest[n..];
            }
            _ => {}
        }
        // Offsets: `+N`, `-N`, `+`, `-`, or a bare number after an address.
        loop {
            let t = skip_ws(rest);
            let Some(c) = t.chars().next() else { break };
            let (sign, body) = match c {
                '+' => (1i64, &t[1..]),
                '-' => (-1i64, &t[1..]),
                d if d.is_ascii_digit() && line.is_some() => (1i64, t),
                _ => break,
            };
            let n = body
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(body.len());
            let v: i64 = if n == 0 {
                1
            } else {
                body[..n].parse().unwrap_or(i64::MAX / 4)
            };
            rest = &body[n..];
            let b = line.unwrap_or(base as i64);
            line = Some(b.saturating_add(sign * v));
        }
        match line {
            None => Ok((None, rest)),
            Some(l) if l < 0 || l > last as i64 => Err("E16: Invalid range".into()),
            Some(l) => Ok((Some(l as usize), rest)),
        }
    }

    fn visual_lines_exact(&self, start: bool) -> ExRes<(usize, usize)> {
        match self.doc().last_visual {
            Some(v) => {
                let (a, b) = if v.start <= v.end {
                    (v.start, v.end)
                } else {
                    (v.end, v.start)
                };
                let _ = start;
                Ok((a.line + 1, b.line + 1))
            }
            None => Err("E20: Mark not set".into()),
        }
    }

    /// The line found by searching for `pat` from line `base` (1-based).
    fn addr_search(&mut self, pat: &str, fwd: bool, base: usize) -> ExRes<usize> {
        let pattern = if pat.is_empty() {
            self.last_pat
                .clone()
                .ok_or("E35: No previous regular expression")?
        } else {
            self.search.pat = Some(pat.to_string());
            self.last_pat = Some(pat.to_string());
            self.no_hlsearch = false;
            crate::cmdline::push_history(&mut self.search_hist, pat);
            pat.to_string()
        };
        let re = Regex::new(&pattern, &self.re_opts())?;
        let line = base.clamp(1, self.buf().line_count()) - 1;
        let start = if fwd {
            Pos::new(line, usize::MAX / 4)
        } else {
            Pos::new(line, 0)
        };
        let fl = SearchFlags {
            start: false,
            end: false,
            wrapscan: self.eff().wrapscan(),
        };
        match searchit(self.buf(), &re, Start::At(start), fwd, 1, &fl) {
            Some(f) => Ok(f.pos.line + 1),
            None => Err(format!("E486: Pattern not found: {pattern}")),
        }
    }

    // ---- files ----------------------------------------------------------

    fn cmd_write(&mut self, p: &Parsed, then_quit: bool) {
        let target: PathBuf = if p.arg.is_empty() {
            match &self.buf().path {
                Some(path) => path.clone(),
                None => return self.error("E32: No file name"),
            }
        } else {
            PathBuf::from(p.arg)
        };
        let own_file = self.buf().path.as_deref() == Some(target.as_path());
        if !own_file && !p.bang && target.exists() {
            return self.error("E13: File exists (add ! to override)");
        }
        // With the agent opt-in, `:w` refuses to overwrite a file that changed
        // since it was read, or text with conflict markers (conflict.rs).
        let guard = if own_file && !p.bang && self.agent_enabled {
            self.write_guard(&target)
        } else {
            None
        };
        if let Some(msg) = guard {
            return self.error(msg);
        }
        let is_new = !target.exists();
        match self.buf().write_to(&target) {
            Ok(stats) => {
                if self.buf().path.is_none() {
                    self.buf_mut().path = Some(target.clone());
                }
                if self.buf().path.as_deref() == Some(target.as_path()) {
                    self.mark_saved();
                    // The disk state of what was written (echo suppression).
                    // The modification time is taken after the write; the hash
                    // is of the bytes `write_to` wrote.
                    let mtime = fs::metadata(&target).ok().and_then(|m| m.modified().ok());
                    let disk = DiskState::of(mtime, &self.buf().to_bytes());
                    let lines = self.buf().lines();
                    self.doc_mut().baseline = Some(lines);
                    self.doc_mut().seen = None;
                    self.doc_mut().disk = Some(disk);
                    self.doc_mut().stale = None;
                    self.doc_mut().warned = None;
                }
                let new = if is_new { " [New]" } else { "" };
                self.info(format!(
                    "\"{}\"{new} {} written",
                    target.display(),
                    describe(stats, self.buf().crlf)
                ));
                if then_quit {
                    self.quit = true;
                }
            }
            Err(e) => self.error(format!(
                "E212: Can't open file for writing: {}: {e}",
                target.display()
            )),
        }
    }

    fn cmd_edit(&mut self, p: &Parsed) {
        if self.buf().modified && !p.bang {
            return self.error("E37: No write since last change (add ! to override)");
        }
        let path = if p.arg.is_empty() {
            match &self.buf().path {
                Some(path) => path.clone(),
                None => return self.error("E32: No file name"),
            }
        } else {
            PathBuf::from(p.arg)
        };
        self.edit_file(Path::new(&path));
    }

    /// `:checktime`: compares the current file with what the buffer was last
    /// read from or written to (Vim's `checktime`). Nothing is said when the
    /// file is as it was. A changed file is reloaded as one undo step tagged
    /// `external` when the buffer is unmodified and 'autoread' is on; otherwise
    /// the buffer is left alone and Vim's warning says so (W11, W12, W13), and
    /// the document is marked stale. A file that is gone gives E211.
    ///
    /// This reads the file synchronously, which is acceptable because the
    /// user asked for it. The file watcher (PLAN.md, milestone 7 step 2b) does
    /// the same reads on a worker thread, not here.
    fn ex_checktime(&mut self, arg: &str) -> ExRes<()> {
        // The argument is a buffer address, as in Vim (probed with Vim 9.1.1914):
        // a number is a buffer number, and a missing buffer is silently
        // skipped; `0` is E939; a name matches a buffer name, else E94.
        // rvim has one buffer, numbered 1, and no alternate buffer.
        let arg = arg.trim();
        if arg.is_empty() || arg == "%" {
            return self.checktime_current();
        }
        if arg.bytes().all(|b| b.is_ascii_digit()) {
            return match arg.parse::<u64>() {
                Ok(0) => Err("E939: Positive count required: checktime 0".into()),
                Ok(1) => self.checktime_current(),
                // A number of a buffer that does not exist: nothing to check.
                _ => Ok(()),
            };
        }
        let matches_name = self
            .buf()
            .path
            .as_ref()
            .is_some_and(|p| p.display().to_string().contains(arg));
        if matches_name {
            return self.checktime_current();
        }
        Err(format!("E94: No matching buffer for {arg}"))
    }

    /// The check of the current buffer's file (see `ex_checktime`).
    fn checktime_current(&mut self) -> ExRes<()> {
        let Some(path) = self.buf().path.clone() else {
            return Ok(());
        };
        let read = match read_file(&path) {
            Ok(read) => read,
            Err(e) => {
                let name = path.display();
                return Err(format!("E211: File \"{name}\" no longer available ({e})"));
            }
        };
        let now = read.map(|(mtime, bytes)| (DiskState::of(mtime, &bytes), bytes));
        let doc = self.win().doc;
        match self.check_disk(
            doc,
            now.as_ref().map(|(s, b)| (s.clone(), b.as_slice())),
            false,
        ) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// The effective value of 'autoread' for the current document and window.
    /// It is global-local: an unset local value means the global one.
    pub(crate) fn autoread_on(&self) -> bool {
        let i = crate::options::Options::find("autoread");
        i.is_some_and(|i| matches!(self.eff().value(i), crate::options::Val::Bool(true)))
    }

    // ---- line commands --------------------------------------------------

    fn check_range_lines(&self, r: Range) -> ExRes<()> {
        if r.line1 < 1 || r.line2 > self.buf().line_count() || r.line1 > r.line2 {
            return Err("E16: Invalid range".into());
        }
        Ok(())
    }

    /// `:[range]d [x] [count]` and `:[range]y [x] [count]`.
    fn ex_delete_yank(&mut self, mut r: Range, arg: &str, delete: bool) -> ExRes<()> {
        let mut reg = None;
        let mut rest = arg.trim();
        if let Some(c) = rest.chars().next()
            && !c.is_ascii_digit()
            && crate::register::valid_name(c)
        {
            reg = Some(c);
            rest = rest[c.len_utf8()..].trim_start();
        }
        if let Some(c) = rest.chars().next() {
            if c.is_ascii_digit() {
                let n: usize = rest
                    .trim_end()
                    .parse()
                    .map_err(|_| "E488: Trailing characters".to_string())?;
                if n == 0 {
                    return Err("E939: Positive count required".into());
                }
                r.line1 = r.line2;
                r.line2 = (r.line2 + n - 1).min(self.buf().line_count());
            } else {
                return Err(format!("E488: Trailing characters: {rest}"));
            }
        }
        if r.line1 == 0 {
            r.line1 = 1;
        }
        self.check_range_lines(r)?;
        if self.buf().has_no_lines() && delete {
            return Ok(());
        }
        let saved = self.win().cursor;
        self.win_mut().cursor = Pos::new(r.line1 - 1, 0);
        self.win_mut().cursor.col = first_non_blank(self.buf(), self.win().cursor.line, true);
        let mut oa = self.line_oparg(if delete { Op::Delete } else { Op::Yank }, r, reg);
        if delete {
            self.op_delete(&mut oa);
            self.win_mut().keep_curswant = false;
        } else {
            self.op_yank(&oa, false, true);
            // The cursor does not move for :yank.
            self.win_mut().cursor = saved;
        }
        Ok(())
    }

    fn line_oparg(&self, op: Op, r: Range, reg: Option<char>) -> OpArg {
        OpArg {
            op,
            reg,
            start: Pos::new(r.line1 - 1, 0),
            end: Pos::new(r.line2 - 1, 0),
            linewise: true,
            inclusive: false,
            block: false,
            start_vcol: 0,
            end_vcol: 0,
            line_count: r.line2 - r.line1 + 1,
            is_visual: false,
            empty: false,
            use_reg_one: false,
            force: None,
            is_max: false,
        }
    }

    /// `:[line]pu[!] [x]`.
    fn ex_put(&mut self, r: Range, before: bool, arg: &str) -> ExRes<()> {
        let name = arg.trim().chars().next();
        self.regs.select(name, false);
        let Some(reg) = self.regs.current().cloned() else {
            return Err(format!(
                "E353: Nothing in register {}",
                name.map_or("\"".to_string(), |c| c.to_string())
            ));
        };
        let lines: Vec<String> = reg.lines.clone();
        // Line 0 puts above the first line.
        let (at, _) = if r.line2 == 0 {
            (0, true)
        } else if before {
            (r.line2 - 1, false)
        } else {
            (r.line2, true)
        };
        self.u_save(at, 0, false);
        let n = lines.len();
        for (i, l) in lines.iter().enumerate() {
            self.buf_mut().insert_line(at + i, l);
        }
        self.win_mut().cursor.line = at + n - 1;
        self.win_mut().cursor.col = first_non_blank(self.buf(), self.win().cursor.line, true);
        self.doc_mut().uline = None;
        // Vim's do_put() for a linewise put: `'[` is the first new line and
        // `']` the last character of the last one.
        if let Some(last_text) = lines.last() {
            self.doc_mut().op_start = Some(Pos::new(at, 0));
            self.doc_mut().op_end = Some(Pos::new(
                at + n - 1,
                last_text.chars().count().saturating_sub(1),
            ));
        }
        if n > 2 {
            self.info(format!("{n} more lines"));
        }
        let _ = reg.kind;
        Ok(())
    }

    /// `:[line]r[ead] [file]`: inserts the lines of `file` (default: the
    /// current file) below `line`; line 0 inserts above the first line. The
    /// cursor goes to the last new line (on its first non-blank), as Vim's
    /// `readfile()` does, or stays on the insert line for an empty file.
    fn ex_read(&mut self, r: Range, bang: bool, arg: &str) -> ExRes<()> {
        if bang || arg.starts_with('!') {
            return Err("rvim: :read !{cmd} is not supported".into());
        }
        let path = if arg.is_empty() {
            self.buf().path.clone().ok_or("E32: No file name")?
        } else {
            PathBuf::from(arg)
        };
        if r.line2 > self.buf().line_count() {
            return Err("E16: Invalid range".into());
        }
        let bytes =
            fs::read(&path).map_err(|_| format!("E484: Can't open file {}", path.display()))?;
        let (lines, stats) = Buffer::file_lines(&bytes);
        // 0-based index of the first new line.
        let at = r.line2;
        self.u_save(at, 0, false);
        for (i, l) in lines.iter().enumerate() {
            self.buf_mut().insert_line(at + i, l);
        }
        let last = if lines.is_empty() {
            r.line2.max(1) - 1
        } else {
            at + lines.len() - 1
        };
        self.win_mut().cursor.line = last;
        self.win_mut().cursor.col = first_non_blank(self.buf_mut(), last, true);
        self.doc_mut().uline = None;
        self.info(format!(
            "\"{}\" {}L, {}B",
            path.display(),
            stats.lines,
            stats.bytes
        ));
        Ok(())
    }

    /// Parses the destination of `:m` / `:t`.
    fn copymove_dest(&mut self, arg: &str) -> ExRes<usize> {
        let (addr, rest) = self.get_address(arg, self.win().cursor.line + 1)?;
        if !skip_ws(rest).is_empty() {
            return Err(format!("E488: Trailing characters: {rest}"));
        }
        addr.ok_or_else(|| "E14: Invalid address".to_string())
    }

    fn ex_copymove(&mut self, r: Range, arg: &str, is_move: bool) -> ExRes<()> {
        self.check_range_lines(r)?;
        let dest = self.copymove_dest(arg)?;
        let n = r.line2 - r.line1 + 1;
        if is_move {
            if dest >= r.line1 && dest < r.line2 {
                return Err("E134: Cannot move a range of lines into itself".into());
            }
            if dest == r.line2 || dest + 1 == r.line1 {
                // Already there: the cursor still moves.
                self.win_mut().cursor.line = (if dest >= r.line2 { dest } else { r.line2 }) - 1;
                self.win_mut().cursor.line = r.line2 - 1;
                self.win_mut().cursor.col =
                    first_non_blank(self.buf(), self.win().cursor.line, true);
                return Ok(());
            }
            let moved: Vec<String> = (r.line1 - 1..r.line2)
                .map(|l| self.buf().line_string(l))
                .collect();
            self.u_save(0, self.buf().line_count(), false);
            // Marks on the moved lines go with them (Vim's do_move()): they
            // are taken out here and put back where the lines land.
            let first = r.line1 - 1;
            let carried = self.buf_mut().take_marks_in(first, r.line2 - 1);
            let pos_mark = self.buf().pos_edit_count();
            if dest >= r.line2 {
                for (i, l) in moved.iter().enumerate() {
                    self.buf_mut().insert_line(dest + i, l);
                }
                self.buf_mut().delete_lines(r.line1 - 1, n);
                self.win_mut().cursor.line = dest - 1;
            } else {
                self.buf_mut().delete_lines(r.line1 - 1, n);
                for (i, l) in moved.iter().enumerate() {
                    self.buf_mut().insert_line(dest + i, l);
                }
                self.win_mut().cursor.line = dest + n - 1;
            }
            let new_first = if dest >= r.line2 { dest - n } else { dest };
            // The jumplist and the context mark follow the move as one
            // mapping: the moved block lands at `new_first`, the lines
            // between the two places shift the other way.
            let (a, b, down) = (first, r.line2 - 1, dest >= r.line2);
            self.buf_mut().replace_pos_edits_from(
                pos_mark,
                PosEdit::Remap(Box::new(move |p: Pos| {
                    let l = p.line;
                    let l = if (a..=b).contains(&l) {
                        l - a + new_first
                    } else if down && l > b && l < dest {
                        l - n
                    } else if !down && l >= dest && l < a {
                        l + n
                    } else {
                        l
                    };
                    Pos::new(l, p.col)
                })),
            );
            for (mark, p) in carried {
                self.buf_mut()
                    .set_mark(mark, Pos::new(new_first + p.line - first, p.col));
            }
            // Vim's do_move(): `'[` and `']` are the lines the moved lines
            // land on, at column 0.
            let (lo, hi) = if dest >= r.line2 {
                (dest - n + 1, dest)
            } else {
                (dest + 1, dest + n)
            };
            self.doc_mut().op_start = Some(Pos::new(lo - 1, 0));
            self.doc_mut().op_end = Some(Pos::new(hi - 1, 0));
            if n > 2 {
                self.info(format!("{n} lines moved"));
            }
        } else {
            let copied: Vec<String> = (r.line1 - 1..r.line2)
                .map(|l| self.buf().line_string(l))
                .collect();
            self.u_save(dest, 0, false);
            for (i, l) in copied.iter().enumerate() {
                self.buf_mut().insert_line(dest + i, l);
            }
            self.win_mut().cursor.line = dest + n - 1;
            // Vim's ex_copy(): `'[` and `']` are the new lines, at column 0.
            self.doc_mut().op_start = Some(Pos::new(dest, 0));
            self.doc_mut().op_end = Some(Pos::new(dest + n - 1, 0));
            if n > 2 {
                self.info(format!("{n} more lines"));
            }
        }
        self.doc_mut().uline = None;
        self.win_mut().cursor.col = first_non_blank(self.buf(), self.win().cursor.line, true);
        Ok(())
    }

    /// `:jumps`: the jumplist on one line, oldest first, with `>` before the
    /// entry `<C-o>` counts from (a message, not Vim's listing).
    fn ex_jumps(&mut self) {
        self.sync_jumps();
        self.win_mut().jumps.cleanup();
        let (entries, idx) = self.win().jumps.entries();
        let mut items: Vec<String> = entries
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mark = if i == idx { ">" } else { "" };
                format!("{mark}{}:{}", p.line + 1, p.col + 1)
            })
            .collect();
        if idx == entries.len() {
            items.push(">".to_string());
        }
        self.info(format!("jumps: {}", items.join(" ")));
    }

    /// `:[range]k{a-z}` and `:[range]mark {a-z}`: sets the mark on the last
    /// line of the range, at its first non-blank (Vim's ex_mark()).
    fn ex_mark(&mut self, r: Range, arg: &str) -> ExRes<()> {
        let arg = arg.trim_end();
        let mut chars = arg.chars();
        let Some(c) = chars.next() else {
            return Err("E471: Argument required".into());
        };
        if chars.next().is_some() {
            return Err(format!("E488: Trailing characters: {arg}"));
        }
        if !c.is_ascii_alphabetic() {
            return Err("E191: Argument must be a letter or forward/backward quote".into());
        }
        let line = r.line2 - 1;
        let col = first_non_blank(self.buf(), line, true);
        self.set_named_mark(c, Pos::new(line, col));
        // Vim's beginline() on the temporary cursor sets the desired column,
        // so `:k` resets it even though the cursor does not move.
        self.win_mut().keep_curswant = false;
        Ok(())
    }

    fn ex_join(&mut self, mut r: Range, bang: bool, arg: &str) -> ExRes<()> {
        self.win_mut().cursor.line = (r.line1 - 1).min(self.buf_mut().last_line());
        let arg = arg.trim();
        if let Ok(n) = arg.parse::<usize>() {
            if n == 0 {
                return Err("E939: Positive count required".into());
            }
            r.line1 = r.line2;
            r.line2 = r.line2 + n - 1;
            r.count = 2;
        }
        if r.line1 == r.line2 {
            if r.count >= 2 {
                return Ok(());
            }
            if r.line2 == self.buf().line_count() {
                self.beep();
                return Ok(());
            }
            r.line2 += 1;
        }
        r.line2 = r.line2.min(self.buf().line_count());
        if r.line1 == r.line2 {
            return Ok(());
        }
        self.win_mut().cursor = Pos::new(r.line1 - 1, 0);
        self.do_join(r.line2 - r.line1 + 1, !bang, true);
        self.win_mut().cursor.col = first_non_blank(self.buf(), self.win().cursor.line, true);
        Ok(())
    }

    fn ex_shift(&mut self, mut r: Range, left: bool, arg: &str) -> ExRes<()> {
        let mut rest = arg.trim();
        let mut amount = 1;
        let ch = if left { '<' } else { '>' };
        while let Some(s) = rest.strip_prefix(ch) {
            amount += 1;
            rest = s.trim_start();
        }
        if let Ok(n) = rest.parse::<usize>() {
            r.line1 = r.line2;
            r.line2 = (r.line2 + n - 1).min(self.buf().line_count());
        } else if !rest.is_empty() {
            return Err(format!("E488: Trailing characters: {rest}"));
        }
        self.check_range_lines(r)?;
        self.win_mut().cursor = Pos::new(r.line1 - 1, 0);
        self.win_mut().cursor.col = first_non_blank(self.buf(), self.win().cursor.line, true);
        let oa = self.line_oparg(if left { Op::LShift } else { Op::RShift }, r, None);
        self.op_shift(&oa, false, amount);
        self.win_mut().cursor.line = r.line2 - 1;
        self.win_mut().cursor.col = first_non_blank(self.buf(), self.win().cursor.line, true);
        Ok(())
    }

    // ---- :normal and :g -------------------------------------------------

    fn ex_normal(&mut self, r: Range, keys: &str) {
        let ks: Vec<Key> = keys.chars().map(char_to_key).collect();
        let run = |ed: &mut Editor| {
            for &k in &ks {
                ed.handle_key(k);
            }
            // An unfinished command is aborted.
            let mut guard = 0;
            while (ed.mode != Mode::Normal || !ed.pending.is_empty()) && guard < 5 {
                ed.handle_key(Key::Esc);
                guard += 1;
            }
        };
        if r.count == 0 {
            run(self);
        } else {
            let mut l = r.line1.max(1);
            loop {
                self.win_mut().cursor = Pos::new((l - 1).min(self.buf_mut().last_line()), 0);
                run(self);
                l += 1;
                if l > r.line2 {
                    break;
                }
            }
        }
    }

    fn ex_global(&mut self, r: Range, invert: bool, arg: &str) -> ExRes<()> {
        if self.global_busy {
            return Err("E147: Cannot do :global recursive with a range".into());
        }
        let (l1, l2) = if r.count == 0 {
            (1, self.buf().line_count())
        } else {
            (r.line1, r.line2)
        };
        let mut chars = arg.chars();
        let Some(delim) = chars.next() else {
            return Err("E35: No previous regular expression".into());
        };
        if delim.is_alphanumeric() || delim == '\\' || delim == '"' || delim == '|' {
            return Err("E146: Regular expressions can't be delimited by letters".into());
        }
        let (pat, rest) = split_at_delim(&arg[delim.len_utf8()..], delim);
        let cmd = rest.unwrap_or("");
        let pattern = if pat.is_empty() {
            self.last_pat
                .clone()
                .ok_or("E35: No previous regular expression")?
        } else {
            self.search.pat = Some(pat.clone());
            self.last_pat = Some(pat.clone());
            self.no_hlsearch = false;
            crate::cmdline::push_history(&mut self.search_hist, &pat);
            pat
        };
        let re = Regex::new(&pattern, &self.re_opts())?;
        let marked: Vec<usize> = (l1 - 1..l2)
            .filter(|&l| re.is_match(self.buf(), l) != invert)
            .collect();
        if marked.is_empty() {
            let msg = if invert {
                format!("Pattern found in every line: {pattern}")
            } else {
                format!("Pattern not found: {pattern}")
            };
            self.info(msg);
            return Ok(());
        }
        let cmd = if cmd.trim().is_empty() { "p" } else { cmd };
        let old_lines = self.buf().line_count() as i64;
        self.buf_mut().tracked = marked;
        // The context mark is set once for the whole command.
        self.set_pcmark();
        self.global_busy = true;
        self.global_need_beginline = false;
        // The lines run as queued tasks, so that a `c` flag prompt can wait
        // for an answer in the middle (see `run_ex_tasks`).
        self.ex_tasks.push(ExTask::GlobalEnd { old_lines });
        self.ex_tasks.push(ExTask::GlobalLines {
            cmd: cmd.to_string(),
            next: 0,
        });
        Ok(())
    }

    // ---- :set, :setlocal, :setglobal -------------------------------------

    fn ex_set(&mut self, arg: &str, target: Target) -> ExRes<()> {
        for item in arg.split_whitespace() {
            self.set_one(item, target)?;
            if crate::options::is_hlsearch_arg(item) {
                self.no_hlsearch = false;
            }
        }
        Ok(())
    }

    /// One argument of `:set`, `:setlocal` or `:setglobal`: `opt`, `noopt`,
    /// `invopt`, `opt!`, `opt?`, `opt&`, `opt<`, `opt=val`, `opt:val`,
    /// `opt+=val`, `opt-=val`, `opt^=val`. The target says which value a
    /// change writes (`options.rs`).
    pub(crate) fn set_one(&mut self, item: &str, target: Target) -> ExRes<()> {
        use crate::options::{INERT, Options, TABLE, Val};
        let name_end = item
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(item.len());
        let (name, rest) = item.split_at(name_end);
        if INERT.contains(&name) {
            return Ok(());
        }
        let mut name = name;
        let mut prefix = "";
        if Options::find(name).is_none() {
            for p in ["no", "inv"] {
                if let Some(b) = name.strip_prefix(p)
                    && (Options::find(b).is_some() || INERT.contains(&b))
                {
                    name = b;
                    prefix = p;
                    break;
                }
            }
            if INERT.contains(&name) {
                return Ok(());
            }
        }
        let Some(i) = Options::find(name) else {
            return Err(format!("E518: Unknown option: {item}"));
        };
        let bad = || format!("E474: Invalid argument: {item}");
        let is_bool = matches!(TABLE[i].default, Val::Bool(_));
        // `opt` alone shows a number; `noopt` for a number is an error.
        if rest == "?" || (rest.is_empty() && !is_bool && prefix.is_empty()) {
            let text = self.option_shown(i, target);
            self.info(text);
            return Ok(());
        }
        if rest == "<" {
            self.reset_option(i, target);
            return Ok(());
        }
        if is_bool {
            // Vim inverts the raw value with `^ 1`, so an unset local value
            // (-1) becomes -2, which is still a set (negative) value.
            let cur = self.option_base(i, target);
            let new = match (prefix, rest) {
                ("no", "") => 0,
                ("inv", "") | ("", "!") => cur ^ 1,
                ("", "") => 1,
                ("", "&" | "&vim") => match TABLE[i].default {
                    Val::Bool(d) => i64::from(d),
                    _ => 0,
                },
                _ => return Err(bad()),
            };
            self.write_option(i, target, new);
            return Ok(());
        }
        if !prefix.is_empty() {
            return Err(bad());
        }
        let cur = self.option_base(i, target);
        let (op, val) = if let Some(v) = rest.strip_prefix("+=") {
            ('+', v)
        } else if let Some(v) = rest.strip_prefix("-=") {
            ('-', v)
        } else if let Some(v) = rest.strip_prefix("^=") {
            ('^', v)
        } else if let Some(v) = rest.strip_prefix(['=', ':']) {
            ('=', v)
        } else if rest == "&" || rest == "&vim" {
            ('&', "")
        } else {
            return Err(bad());
        };
        let n = if op == '&' {
            match TABLE[i].default {
                Val::Num(d) => d,
                _ => 0,
            }
        } else {
            let v: i64 = val
                .parse()
                .map_err(|_| format!("E521: Number required after =: {item}"))?;
            match op {
                '+' => cur + v,
                '-' => cur - v,
                '^' => cur * v,
                _ => v,
            }
        };
        if TABLE[i].name == "numberwidth" {
            // Vim clamps 'numberwidth' to 1 to 20 without an error (probed).
            self.write_option(i, target, n.clamp(1, 20));
            return Ok(());
        }
        let def_name = TABLE[i].name;
        if def_name == "tabstop" && n < 1 {
            // Vim falls back to the default instead of keeping the value.
            self.write_option(i, target, 8);
            return Err(format!("E487: Argument must be positive: {item}"));
        }
        // 'softtabstop' and 'scrolloff' may be negative (Vim accepts them);
        // a negative 'softtabstop' means 'shiftwidth'.
        if n < 0 && !matches!(def_name, "softtabstop" | "scrolloff") {
            return Err(format!("E487: Argument must be positive: {item}"));
        }
        self.write_option(i, target, n);
        Ok(())
    }

    // ---- :substitute ----------------------------------------------------

    fn ex_substitute(&mut self, r: Range, arg: &str, repeat: bool) -> ExRes<()> {
        let mut r = r;
        let mut flags = self.sub_flags.clone();
        let arg = arg.trim_start();
        let pattern: String;
        let rep: String;
        let mut rest: &str;
        let first = arg.chars().next();
        let is_repeat = repeat
            || match first {
                None => true,
                Some(c) => {
                    c.is_ascii_digit() || "&cegiInp#lr".contains(c) && !c.is_ascii_punctuation()
                }
            };
        if is_repeat {
            let Some(p) = self.last_pat.clone() else {
                return Err("E35: No previous regular expression".into());
            };
            pattern = p;
            rep = self.last_sub_raw.clone();
            rest = arg;
            if repeat && arg.starts_with('&') {
                rest = &arg[1..];
                // `:&&` keeps the flags.
            } else {
                flags = SubFlags::default();
            }
        } else {
            let delim = first.unwrap();
            if delim.is_alphanumeric() || matches!(delim, '\\' | '"' | '|') {
                return Err("E146: Regular expressions can't be delimited by letters".into());
            }
            let body = &arg[delim.len_utf8()..];
            let (pat, after) = split_at_delim_keep(body, delim);
            let Some(after) = after else {
                // `:s/pat` without replacement: replaced by nothing.
                pattern = self.pick_pattern(&pat)?;
                rep = String::new();
                return self.do_substitute(r, &pattern, &rep, SubFlags::default());
            };
            pattern = self.pick_pattern(&pat)?;
            // `\=expr` is taken up to the delimiter as typed: no escapes, and
            // no `~` (Vim's do_sub() does not expand it either).
            let (rp, after2) = if after.starts_with("\\=") {
                split_expr_rep(after, delim)
            } else {
                let (rp, after2) = split_rep(after, delim);
                // `~` stands for the previous replacement.
                (expand_tilde(&rp, &self.last_sub_raw), after2)
            };
            rep = rp;
            rest = after2.unwrap_or("");
            flags = SubFlags::default();
        }
        // Flags.
        let mut rest_chars = rest.trim_start();
        if let Some(s) = rest_chars.strip_prefix('&') {
            flags = self.sub_flags.clone();
            rest_chars = s;
        }
        loop {
            match rest_chars.chars().next() {
                Some('g') => flags.global = !flags.global,
                Some('i') => flags.ic = Some(true),
                Some('I') => flags.ic = Some(false),
                Some('e') => flags.no_error = true,
                Some('n') => flags.count_only = true,
                Some('p' | '#' | 'l') => flags.print = true,
                Some('r') => flags.use_search = true,
                Some('c') => flags.confirm = true,
                _ => break,
            }
            rest_chars = &rest_chars[1..];
        }
        rest_chars = rest_chars.trim_start();
        if let Some(d) = rest_chars.chars().next()
            && d.is_ascii_digit()
        {
            let n = rest_chars
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest_chars.len());
            let cnt: usize = rest_chars[..n].parse().unwrap_or(1);
            if cnt == 0 {
                return Err("E939: Positive count required".into());
            }
            r.line1 = r.line2;
            r.line2 = (r.line2 + cnt - 1).min(self.buf().line_count());
        }
        self.do_substitute(r, &pattern, &rep, flags)
    }

    fn pick_pattern(&mut self, pat: &str) -> ExRes<String> {
        if pat.is_empty() {
            self.last_pat
                .clone()
                .ok_or_else(|| "E35: No previous regular expression".to_string())
        } else {
            crate::cmdline::push_history(&mut self.search_hist, pat);
            Ok(pat.to_string())
        }
    }

    fn do_substitute(&mut self, r: Range, pattern: &str, rep: &str, flags: SubFlags) -> ExRes<()> {
        self.check_range_lines(r)?;
        self.last_pat = Some(pattern.to_string());
        self.no_hlsearch = false;
        self.search.pat = Some(pattern.to_string());
        self.last_sub_raw = rep.to_string();
        self.sub_flags = flags.clone();
        let mut opts = self.re_opts();
        if let Some(ic) = flags.ic {
            opts.ignorecase = ic;
            opts.smartcase = false;
        }
        let re = Regex::new(pattern, &opts)?;
        let rep_chars: Vec<char> = rep.chars().collect();
        // The expanded replacement is remembered for `~`.
        self.last_sub = expand_rep_literal(&rep_chars);
        let mut st = Box::new(SubRun {
            pattern: pattern.to_string(),
            rep: rep.to_string(),
            rep_chars,
            ask: flags.confirm && !flags.count_only,
            re,
            line1: r.line1,
            line2: r.line2,
            lnum: r.line1 - 1,
            nsubs: 0,
            nlines: 0,
            last_line: None,
            multiline_pat: re_multiline(pattern),
            saved_cursor: self.win().cursor,
            got_match: false,
            found: false,
            got_quit: false,
            awaiting: false,
            skip_answer: false,
            line: None,
            m: None,
            rest: None,
            error: None,
            flags,
        });
        if self.sub_drive(&mut st)? {
            self.sub = Some(st);
        }
        Ok(())
    }

    /// Runs the substitution until it is done (`Ok(false)`) or waits for a
    /// `c` flag answer (`Ok(true)`: Confirm mode, the prompt and the match
    /// are set). Port of the loops of Vim's `do_sub()`.
    fn sub_drive(&mut self, st: &mut SubRun) -> ExRes<bool> {
        loop {
            if st.line.is_none() {
                if st.got_quit || st.lnum >= st.line2 || st.lnum >= self.buf().line_count() {
                    break;
                }
                let Some(m) = st.re.match_line(self.buf(), st.lnum, 0) else {
                    st.lnum += 1;
                    continue;
                };
                // A line break before \zs: the match starts in a later line.
                if m.start.0 > st.lnum {
                    st.lnum = m.start.0;
                    if st.lnum >= st.line2 {
                        break;
                    }
                }
                st.line = Some(LineSub::new(self.line_vec(st.lnum)));
                st.m = Some(m);
            }
            // The matches of this line.
            loop {
                let m = st.m.clone().expect("current match");
                st.found = true;
                // Where the cursor goes if the command ends here (Vim sets it
                // to the line of each match it looks at).
                st.last_line = Some(st.lnum);
                let mut do_again = false;
                let mut nmatch = m.end.0 - m.start.0 + 1;
                let action = {
                    let ln = st.line.as_mut().expect("line state");
                    // 1. An empty match right after the previous match does
                    // not count (the strange vi behaviour; also avoids
                    // endless loops).
                    let dup = ln.matchcol as i64 == ln.prev_matchcol
                        && m.end.0 == m.start.0
                        && ln.matchcol == m.end.1;
                    if std::mem::take(&mut st.awaiting) {
                        // The answer to the prompt for this match.
                        if std::mem::take(&mut st.skip_answer) {
                            Action::Skip
                        } else {
                            Action::Substitute
                        }
                    } else if dup {
                        if ln.matchcol >= ln.sub_line.len() {
                            ln.skip_match = true;
                        } else {
                            ln.matchcol += 1;
                        }
                        Action::Skip
                    } else {
                        // The context mark is set at the first match, from the
                        // cursor as it was before the command (Vim's
                        // `got_match` in do_sub()).
                        if !st.got_match {
                            st.got_match = true;
                            self.set_pcmark();
                        }
                        // Normally we continue searching just after the
                        // previous match.
                        ln.matchcol = m.end.1;
                        ln.prev_matchcol = ln.matchcol as i64;
                        if st.flags.count_only {
                            if nmatch > 1 {
                                ln.matchcol = ln.sub_line.len();
                                nmatch = 1;
                                ln.skip_match = true;
                            }
                            st.nsubs += 1;
                            ln.did_sub = true;
                            Action::Skip
                        } else if st.ask {
                            // Like Vim, the cursor goes to the match asked about.
                            st.awaiting = true;
                            self.win_mut().cursor = Pos::new(st.lnum, m.start.1);
                            self.message = Some(Message {
                                text: format!("replace with {} (y/n/a/q/l/^E/^Y)?", st.rep),
                                error: false,
                            });
                            self.mode = Mode::Confirm;
                            return Ok(true);
                        } else {
                            Action::Substitute
                        }
                    }
                };
                // `\=expr` is evaluated for this match. An error ends the `:s`:
                // from a function the match stays as it is, otherwise it is
                // replaced by nothing (as in Vim's vim_regsub()).
                let mut expr_text: Option<String> = None;
                if action == Action::Substitute && st.rep_chars.starts_with(&['\\', '=']) {
                    match self.eval_sub_expr(&st.rep_chars[2..], &m, st.lnum) {
                        Ok((text, errs)) => {
                            expr_text = Some(text);
                            st.error = errs.into_iter().last();
                        }
                        Err(e) => {
                            expr_text = Some(String::new());
                            st.error = Some(e.msg);
                        }
                    }
                }
                // 3. Substitute the string. Like Vim, the cursor then sits at
                // the start of the line (also where undo returns to).
                if action == Action::Substitute {
                    self.win_mut().cursor = Pos::new(st.lnum, 0);
                    let out = match expr_text {
                        Some(text) => text
                            .chars()
                            .map(|c| match c {
                                '\n' | '\r' => RepPiece::Break,
                                c => RepPiece::Text(c),
                            })
                            .collect(),
                        None => self.expand_replacement(&st.rep_chars, &m),
                    };
                    let ln = st.line.as_mut().expect("line state");
                    let mut ns = ln.new_start.take().unwrap_or_default();
                    let sc = m.start.1.min(ln.sub_line.len());
                    if sc >= ln.copycol {
                        ns.extend(&ln.sub_line[ln.copycol..sc]);
                    }
                    for piece in out {
                        match piece {
                            RepPiece::Text(c) => ns.push(c),
                            RepPiece::Break => {
                                ln.out_lines.push(ns.iter().collect());
                                ns.clear();
                                ln.last_cut = (sc, m.end.1);
                            }
                        }
                    }
                    ln.new_start = Some(ns);
                    st.nsubs += 1;
                    ln.did_sub = true;
                    st.last_line = Some(st.lnum + ln.out_lines.len());
                    if nmatch > 1 {
                        // The match joined lines: replace them now and
                        // continue in the joined line.
                        let last = self.buf().line_count() - 1;
                        let end_l = m.end.0.min(last);
                        if m.end.0 > last {
                            ln.skip_match = true;
                        }
                        let tail: Vec<char> = if m.end.0 <= last {
                            self.line_vec(m.end.0)
                        } else {
                            Vec::new()
                        };
                        let mut produced = ln.new_start.take().unwrap_or_default();
                        let plen = produced.len();
                        produced.extend(&tail[m.end.1.min(tail.len())..]);
                        let n_old = end_l - st.lnum + 1;
                        let mut new_lines = std::mem::take(&mut ln.out_lines);
                        new_lines.push(produced.iter().collect());
                        self.u_save_line_start(st.lnum, n_old);
                        // Marks on the joined lines are kept, as in Vim's
                        // do_sub(): a mark on the first line stays, the
                        // others move to the joined line.
                        let last_new = st.lnum + new_lines.len() - 1;
                        let moved: Vec<(char, Pos)> = self
                            .buf_mut()
                            .take_marks_in(st.lnum, end_l)
                            .into_iter()
                            .map(|(c, p)| {
                                let at = if p.line == st.lnum {
                                    p
                                } else if p.line == end_l && p.col >= m.end.1 {
                                    Pos::new(last_new, plen + p.col - m.end.1)
                                } else {
                                    Pos::new(last_new, plen)
                                };
                                (c, at)
                            })
                            .collect();
                        // Insert the new lines first so that the buffer
                        // never becomes empty.
                        for (i, l) in new_lines.iter().enumerate() {
                            self.buf_mut().insert_line(st.lnum + i, l);
                        }
                        self.buf_mut()
                            .delete_lines(st.lnum + new_lines.len(), n_old);
                        for (c, p) in moved {
                            self.buf_mut().set_mark(c, p);
                        }
                        let added = new_lines.len();
                        st.line2 = (st.line2 + added).saturating_sub(n_old);
                        st.lnum += added - 1;
                        st.last_line = Some(st.lnum);
                        ln.sub_line = self.line_vec(st.lnum);
                        ln.copycol = 0;
                        ln.matchcol = plen;
                        ln.prev_matchcol = plen as i64;
                        if st.lnum < st.line2 {
                            do_again = true;
                        }
                    } else {
                        ln.copycol = m.end.1;
                    }
                }
                // skip:
                let ln = st.line.as_ref().expect("line state");
                let lastone = ln.skip_match
                    || st.got_quit
                    || !(st.flags.global || do_again)
                    || (ln.matchcol >= ln.sub_line.len() && nmatch <= 1 && !st.multiline_pat);
                if lastone {
                    break;
                }
                let (lnum, col) = (st.lnum, ln.matchcol);
                match st.re.match_line(self.buf(), lnum, col) {
                    Some(n) if n.start.0 == lnum => st.m = Some(n),
                    _ => break,
                }
            }
            // The line is done: write it back (the text after the last match
            // is kept).
            let ln = st.line.take().expect("line state");
            if ln.did_sub {
                st.nlines += 1;
                if !st.flags.count_only
                    && let Some(mut ns) = ln.new_start
                {
                    if ln.copycol <= ln.sub_line.len() {
                        ns.extend(&ln.sub_line[ln.copycol..]);
                    }
                    self.u_save_line_start(st.lnum, 1);
                    let added = ln.out_lines.len();
                    let pos_mark = self.buf().pos_edit_count();
                    // The length of the text after the last break (the tail
                    // without the rest of the line).
                    let rest = ln.sub_line.len().saturating_sub(ln.copycol);
                    let ylen = ns.len().saturating_sub(rest);
                    self.buf_mut()
                        .set_line(st.lnum, &ns.iter().collect::<String>());
                    for (i, l) in ln.out_lines.iter().enumerate() {
                        self.buf_mut().insert_line(st.lnum + i, l);
                    }
                    if added > 0 {
                        // The first piece stays on this line and the tail
                        // moves to the line after the pieces (Vim's split at
                        // a line break): positions before the match stay,
                        // positions after it move to the tail.
                        let at = st.lnum;
                        let (cut_s, cut_e) = ln.last_cut;
                        self.buf_mut().replace_pos_edits_from(
                            pos_mark,
                            PosEdit::Remap(Box::new(move |p: Pos| {
                                if p.line > at {
                                    Pos::new(p.line + added, p.col)
                                } else if p.line < at || p.col < cut_s {
                                    p
                                } else if p.col >= cut_e {
                                    Pos::new(at + added, p.col - cut_e + ylen)
                                } else {
                                    Pos::new(at, cut_s)
                                }
                            })),
                        );
                    }
                    st.lnum += added;
                    st.line2 += added;
                    st.last_line = Some(st.lnum);
                }
            }
            st.lnum += 1;
        }

        if st.flags.count_only {
            self.win_mut().cursor = st.saved_cursor;
            if st.nsubs > 0 {
                self.info(format!(
                    "{} match{} on {} line{}",
                    st.nsubs,
                    if st.nsubs == 1 { "" } else { "es" },
                    st.nlines,
                    if st.nlines == 1 { "" } else { "s" }
                ));
            } else if !st.found && !st.flags.no_error {
                return Err(format!("E486: Pattern not found: {}", st.pattern));
            }
            return Ok(false);
        }
        if st.nsubs > 0 {
            self.doc_mut().uline = None;
            // Vim's do_sub(): `'[` is the first line of the range and `']`
            // the last, both at column 0.
            self.doc_mut().op_start = Some(Pos::new(st.line1 - 1, 0));
            self.doc_mut().op_end = Some(Pos::new(st.line2.max(1) - 1, 0));
            if self.global_busy {
                self.global_need_beginline = true;
            }
            // While the prompt is still asking, the cursor stays where the
            // last answer left it: on the line of the last match, at the start
            // after a substitution or on the match after `n` (Vim leaves it
            // there). Without a prompt, the cursor goes to the first non-blank
            // of the line of the last match, as for `:s` without `c`.
            if !st.ask
                && let Some(l) = st.last_line
            {
                self.win_mut().cursor.line = l.min(self.buf_mut().last_line());
                self.win_mut().cursor.col = 0;
                if !self.global_busy {
                    self.win_mut().cursor.col =
                        first_non_blank(self.buf(), self.win().cursor.line, true);
                }
            }
            if st.nsubs > 2 {
                self.info(format!(
                    "{} substitution{} on {} line{}",
                    st.nsubs,
                    if st.nsubs == 1 { "" } else { "s" },
                    st.nlines,
                    if st.nlines == 1 { "" } else { "s" }
                ));
            }
        } else if !st.found && !self.global_busy && !st.flags.no_error {
            return Err(format!("E486: Pattern not found: {}", st.pattern));
        }
        if let Some(msg) = st.error.take() {
            self.error(msg);
        }
        Ok(false)
    }

    /// Evaluates the `\=` replacement (`expr`, the text after `\=`) for match
    /// `m` on line `lnum`. As in Vim, the cursor is on the match while the
    /// expression runs, so `line('.')` and `col('.')` see it. The second value
    /// holds the messages of errors inside built-in functions.
    fn eval_sub_expr(
        &self,
        expr: &[char],
        m: &crate::regex::Match,
        lnum: usize,
    ) -> Result<(String, Vec<String>), crate::expr::ExprError> {
        let line = self.line_vec(m.start.0);
        let col: usize = line[..m.start.1.min(line.len())]
            .iter()
            .map(|c| c.len_utf8())
            .sum::<usize>()
            + 1;
        let mut groups: [String; 10] = Default::default();
        for (g, slot) in groups.iter_mut().enumerate() {
            let span = if g == 0 {
                Some((m.start, m.end))
            } else {
                m.groups[g]
            };
            if let Some((a, b)) = span {
                *slot = self.text_between(a, b).iter().collect();
            }
        }
        let env = crate::expr::Env {
            groups,
            line: lnum as i64 + 1,
            col: col as i64,
            last_line: self.buf().line_count() as i64,
            ignorecase: self.eff().ignorecase(),
        };
        crate::expr::eval_text(&expr.iter().collect::<String>(), &env)
    }

    /// An answer to the `c` flag prompt: `y n a q l`, `<Esc>` and `<C-c>`
    /// (quit). `<C-e>` and `<C-y>` scroll the window one line and keep the
    /// prompt. Other keys ask again.
    pub(crate) fn confirm_key(&mut self, key: Key) {
        if let Key::Ctrl(c @ ('e' | 'y')) = key {
            self.scroll_window_for_prompt(c == 'e');
            self.win_mut().keep_curswant = true;
            return;
        }
        let answer = match key {
            Key::Char(c @ ('y' | 'n' | 'a' | 'q' | 'l')) => c,
            Key::Esc | Key::Ctrl('c') => 'q',
            _ => return,
        };
        let Some(mut st) = self.sub.take() else {
            self.mode = Mode::Normal;
            return;
        };
        self.message = None;
        match answer {
            'y' => {}
            'n' => {
                st.skip_answer = true;
                // A match over a line break: go on with the next line.
                let multi = st.m.as_ref().is_some_and(|m| m.end.0 > m.start.0);
                if multi && let Some(ln) = st.line.as_mut() {
                    ln.matchcol = ln.sub_line.len();
                    ln.skip_match = true;
                }
            }
            'a' => st.ask = false,
            'q' => {
                st.got_quit = true;
                st.skip_answer = true;
            }
            // 'l': this match, then stop (Vim sets do_all off and line2 to
            // this line).
            _ => {
                st.flags.global = false;
                st.line2 = st.lnum;
            }
        }
        self.mode = Mode::Normal;
        self.sub_resume(st);
    }

    /// Goes on with a `:s` after its prompt was answered, then the Ex
    /// command line it was part of.
    fn sub_resume(&mut self, mut st: Box<SubRun>) {
        match self.sub_drive(&mut st) {
            Ok(true) => self.sub = Some(st),
            result => {
                if let Err(msg) = result {
                    self.error(msg);
                }
                // As in Vim, an error does not stop the rest of the line.
                if let Some(rest) = st.rest.take() {
                    self.ex_tasks.push(ExTask::Ex(rest));
                }
                self.run_ex_tasks(0);
            }
        }
        // Waiting keeps the desired column; a finished `:s` moves the cursor.
        self.win_mut().keep_curswant = self.sub.is_some();
    }

    /// The match a `c` flag prompt asks about: start and end (exclusive), for
    /// the highlight.
    pub fn confirm_match(&self) -> Option<(Pos, Pos)> {
        if self.mode != Mode::Confirm {
            return None;
        }
        let m = self.sub.as_ref()?.m.as_ref()?;
        Some((Pos::new(m.start.0, m.start.1), Pos::new(m.end.0, m.end.1)))
    }

    /// Vim's `u_save()` for a change at the start of line `first`: the undo
    /// cursor is at column 0, whatever the cursor does during a prompt.
    fn u_save_line_start(&mut self, first: usize, count: usize) {
        let at = self.win().cursor;
        self.win_mut().cursor = Pos::new(first, 0);
        self.u_save(first, count, false);
        self.win_mut().cursor = at;
    }
}

/// What to do with the current match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Substitute,
    Skip,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SubFlags {
    pub global: bool,
    pub ic: Option<bool>,
    pub no_error: bool,
    pub count_only: bool,
    pub print: bool,
    pub use_search: bool,
    /// `c`: ask before each substitution.
    pub confirm: bool,
}

/// A `:s` in progress: the locals of Vim's `do_sub()`, kept between keys
/// while a `c` flag prompt waits for an answer.
pub(crate) struct SubRun {
    /// The context mark was set for this command (at its first match).
    got_match: bool,
    /// The first line of the range (Vim's `eap->line1`), for `'[`.
    line1: usize,
    pattern: String,
    /// The replacement after `~` expansion, as shown in the prompt.
    rep: String,
    rep_chars: Vec<char>,
    flags: SubFlags,
    re: Regex,
    line2: usize,
    lnum: usize,
    nsubs: usize,
    nlines: usize,
    last_line: Option<usize>,
    multiline_pat: bool,
    saved_cursor: Pos,
    /// Vim's `do_ask`: still prompting (`a` turns it off).
    ask: bool,
    /// Some match was found (for E486, which is not given for declined ones).
    found: bool,
    got_quit: bool,
    /// The current match waits for an answer; `skip_answer` skips it.
    awaiting: bool,
    skip_answer: bool,
    /// The line being worked on, while inside a line.
    line: Option<LineSub>,
    /// The current match.
    m: Option<Match>,
    /// The rest of the `|` command line after this `:s`, run when it ends.
    pub(crate) rest: Option<String>,
    /// An error from a `\=` replacement, reported when the `:s` ends.
    error: Option<String>,
}

/// The state of the line a `:s` is working on (Vim's `sub_firstline` and
/// the `copycol`/`matchcol` locals).
struct LineSub {
    /// The line before any replacement in it.
    sub_line: Vec<char>,
    /// The new text up to the last match (`None` until a replacement).
    new_start: Option<Vec<char>>,
    /// Complete new lines, when a replacement holds a line break.
    out_lines: Vec<String>,
    copycol: usize,
    matchcol: usize,
    prev_matchcol: i64,
    did_sub: bool,
    skip_match: bool,
    /// The match (start and end columns) of the last line break, where the
    /// positions of a split line are cut.
    last_cut: (usize, usize),
}

impl LineSub {
    fn new(sub_line: Vec<char>) -> Self {
        Self {
            sub_line,
            new_start: None,
            out_lines: Vec::new(),
            copycol: 0,
            matchcol: 0,
            prev_matchcol: -1,
            did_sub: false,
            skip_match: false,
            last_cut: (0, 0),
        }
    }
}

pub(crate) enum RepPiece {
    Text(char),
    Break,
}

fn re_multiline(pat: &str) -> bool {
    pat.contains("\\n") || pat.contains("\\_")
}

fn is_known_long(name: &str) -> bool {
    matches!(
        name,
        "wq" | "normal"
            | "norm"
            | "norma"
            | "global"
            | "glob"
            | "vglobal"
            | "vglo"
            | "put"
            | "pu"
            | "set"
            | "se"
            | "undo"
            | "redo"
            | "red"
            | "quit"
            | "qa"
            | "qall"
            | "silent"
            | "sil"
            | "sile"
            | "silen"
            | "edit"
            | "ed"
            | "exit"
            | "exi"
            | "xit"
            | "xi"
            | "write"
            | "wr"
            | "wri"
            | "writ"
            | "join"
            | "jo"
            | "joi"
            | "ju"
            | "jum"
            | "jump"
            | "jumps"
            | "marks"
            | "delm"
            | "delma"
            | "delmar"
            | "delmark"
            | "delmarks"
            | "move"
            | "mo"
            | "mov"
            | "ma"
            | "mar"
            | "mark"
            | "copy"
            | "co"
            | "cop"
            | "yank"
            | "ya"
            | "yan"
            | "delete"
            | "de"
            | "del"
            | "dele"
            | "delet"
            | "substitute"
            | "su"
            | "sub"
            | "subs"
            | "subst"
            | "substi"
            | "substit"
            | "substitu"
            | "substitut"
            | "nohlsearch"
            | "noh"
            | "nohl"
            | "nohls"
            | "print"
            | "pr"
            | "pri"
            | "prin"
            | "number"
            | "nu"
            | "num"
            | "numb"
            | "numbe"
            | "list"
            | "version"
            | "ve"
            | "ver"
            | "vers"
            | "versi"
            | "versio"
            | "setlocal"
            | "setloca"
            | "setloc"
            | "setlo"
            | "setl"
            | "setglobal"
            | "setgloba"
            | "setglob"
            | "setglo"
            | "setgl"
            | "setg"
            | "vg"
            | "gl"
            | "glo"
            | "nor"
    )
}

fn is_known_prefix(name: &str) -> bool {
    is_known_long(name)
}

fn char_to_key(c: char) -> Key {
    match c {
        '\x1b' => Key::Esc,
        '\r' | '\n' => Key::Enter,
        '\x08' | '\x7f' => Key::Backspace,
        '\t' => Key::Tab,
        c if (c as u32) < 0x20 => Key::Ctrl((c as u8 + 0x60) as char),
        c => Key::Char(c),
    }
}

/// Like `split_at_delim`, but keeps `\delim` as the pattern needs it
/// (the regex engine reads `\/` as a literal slash anyway).
fn split_at_delim_keep(s: &str, delim: char) -> (String, Option<&str>) {
    split_at_delim(s, delim)
}

/// The byte offset just past the replacement of a `:s` command whose name
/// ends at `name_end`: the pattern and the replacement (`\=expr` included)
/// each end at the first unescaped delimiter, so a `|` inside them is text.
/// `None` when there is no delimiter. An unterminated pattern or replacement
/// runs to the end of the command.
fn sub_args_end(cmd: &str, name_end: usize) -> Option<usize> {
    let delim = cmd[name_end..].chars().next()?;
    if delim.is_alphanumeric() || matches!(delim, '\\' | '"' | '|' | ' ') {
        return None;
    }
    let body = name_end + delim.len_utf8();
    let Some(pat_end) = skip_to_delim(&cmd[body..], delim) else {
        return Some(cmd.len());
    };
    let rep = body + pat_end + delim.len_utf8();
    match skip_to_delim(&cmd[rep..], delim) {
        Some(e) => Some(rep + e + delim.len_utf8()),
        None => Some(cmd.len()),
    }
}

/// The byte offset of the first `delim` in `s` that is not escaped.
fn skip_to_delim(s: &str, delim: char) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut off = 0;
    while i < chars.len() {
        if chars[i] == delim {
            return Some(off);
        }
        let step = if chars[i] == '\\' && i + 1 < chars.len() {
            2
        } else {
            1
        };
        off += chars[i..i + step]
            .iter()
            .map(|c| c.len_utf8())
            .sum::<usize>();
        i += step;
    }
    None
}

/// Splits a `\=` replacement at the delimiter. Nothing is unescaped: an
/// escaped delimiter stays as typed, so the expression sees it.
fn split_expr_rep(s: &str, delim: char) -> (String, Option<&str>) {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == delim {
            let off: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
            return (s[..off].to_string(), Some(&s[off + delim.len_utf8()..]));
        }
        i += if chars[i] == '\\' && i + 1 < chars.len() {
            2
        } else {
            1
        };
    }
    (s.to_string(), None)
}

/// Splits the replacement part at the delimiter. Backslashes are kept for
/// later expansion, except that `\delim` becomes the delimiter.
fn split_rep(s: &str, delim: char) -> (String, Option<&str>) {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == delim {
            let off: usize = chars[..=i].iter().map(|c| c.len_utf8()).sum();
            return (out, Some(&s[off..]));
        }
        if c == '\\' && i + 1 < chars.len() {
            if chars[i + 1] == delim && delim != '\\' && delim != '&' {
                out.push(delim);
            } else {
                out.push(c);
                out.push(chars[i + 1]);
            }
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    (out, None)
}

/// Replaces an unescaped `~` by the previous replacement string.
fn expand_tilde(rep: &str, prev: &str) -> String {
    let mut out = String::new();
    let mut it = rep.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            out.push(c);
            if let Some(n) = it.next() {
                out.push(n);
            }
        } else if c == '~' {
            out.push_str(prev);
        } else {
            out.push(c);
        }
    }
    out
}

/// The replacement with `&` and groups left as typed, for `~`.
fn expand_rep_literal(rep: &[char]) -> String {
    rep.iter().collect()
}

impl Editor {
    /// Vim's `vim_regsub()`: expands `&`, `\1`..`\9`, `\n`, `\r`, `\t`,
    /// `\u \U \l \L \e \E` and `\\` against match `m`.
    fn expand_replacement(&self, rep: &[char], m: &crate::regex::Match) -> Vec<RepPiece> {
        let mut out: Vec<RepPiece> = Vec::new();
        let mut one_shot: Option<bool> = None; // Some(true) = upper next
        let mut mode: Option<bool> = None; // Some(true) = upper until \e
        let mut i = 0;
        let push =
            |out: &mut Vec<RepPiece>, c: char, one: &mut Option<bool>, mode: Option<bool>| {
                let c = match (one.take(), mode) {
                    (Some(true), _) => c.to_uppercase().next().unwrap_or(c),
                    (Some(false), _) => c.to_lowercase().next().unwrap_or(c),
                    (None, Some(true)) => c.to_uppercase().next().unwrap_or(c),
                    (None, Some(false)) => c.to_lowercase().next().unwrap_or(c),
                    _ => c,
                };
                out.push(RepPiece::Text(c));
            };
        while i < rep.len() {
            let c = rep[i];
            i += 1;
            let mut group: Option<usize> = None;
            if c == '&' {
                group = Some(0);
            } else if c == '\\' && i < rep.len() {
                let d = rep[i];
                i += 1;
                match d {
                    '0'..='9' => group = Some(d as usize - '0' as usize),
                    'u' => one_shot = Some(true),
                    'l' => one_shot = Some(false),
                    'U' => mode = Some(true),
                    'L' => mode = Some(false),
                    'e' | 'E' => {
                        mode = None;
                        one_shot = None;
                    }
                    'r' => out.push(RepPiece::Break),
                    'n' => push(&mut out, '\0', &mut one_shot, mode),
                    't' => push(&mut out, '\t', &mut one_shot, mode),
                    '&' => push(&mut out, '&', &mut one_shot, mode),
                    other => push(&mut out, other, &mut one_shot, mode),
                }
            } else if c == '\r' {
                out.push(RepPiece::Break);
            } else {
                push(&mut out, c, &mut one_shot, mode);
            }
            if let Some(g) = group
                && let Some((a, b)) = m.groups[g]
            {
                for ch in self.text_between(a, b) {
                    if ch == '\n' {
                        out.push(RepPiece::Break);
                    } else {
                        push(&mut out, ch, &mut one_shot, mode);
                    }
                }
            }
        }
        out
    }

    fn text_between(&self, a: crate::regex::P, b: crate::regex::P) -> Vec<char> {
        let mut out = Vec::new();
        let mut p = a;
        while p < b {
            let line = self.line_vec_or_empty(p.0);
            if p.1 < line.len() {
                out.push(line[p.1]);
                p.1 += 1;
            } else {
                out.push('\n');
                p = (p.0 + 1, 0);
            }
        }
        out
    }

    fn line_vec_or_empty(&self, l: usize) -> Vec<char> {
        if l < self.buf().line_count() {
            self.line_vec(l)
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use std::fs;

    #[test]
    fn write_quit_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        let mut ed = Editor::from_text("x");
        ed.feed(":w<CR>");
        assert!(ed.message().unwrap().text.starts_with("E32"));
        ed.feed("ihi <Esc>:q<CR>");
        assert!(!ed.should_quit());
        assert!(ed.message().unwrap().text.starts_with("E37"));
        ed.feed(&format!(":w {}<CR>", path.display()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hi x\n");
        assert!(ed.message().unwrap().text.ends_with("[New] 1L, 5B written"));
        ed.feed(":q<CR>");
        assert!(ed.should_quit());
    }

    #[test]
    fn edit_and_goto_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.txt");
        fs::write(&path, "one\n  two\nthree\n").unwrap();
        let mut ed = Editor::new();
        ed.feed(&format!(":e {}<CR>", path.display()));
        assert_eq!(ed.buf().line_count(), 3);
        assert!(ed.message().unwrap().text.ends_with("3L, 16B"));
        ed.feed(":2<CR>");
        assert_eq!((ed.cursor().line, ed.cursor().col), (1, 2));
        ed.feed(":$<CR>");
        assert_eq!(ed.cursor().line, 2);
        ed.feed(":.-2<CR>");
        assert_eq!(ed.cursor().line, 0);
        ed.feed(":ve<CR>");
        // `:version` shows the short notice (the full text is `--version`).
        assert_eq!(ed.message().unwrap().text, crate::NOTICE_SHORT);
        assert!(ed.message().unwrap().text.contains("modified Vim"));
        ed.feed(":frob<CR>");
        assert!(ed.message().unwrap().text.starts_with("E492"));
        ed.feed("x:x<CR>");
        assert!(ed.should_quit());
        assert_eq!(fs::read_to_string(&path).unwrap(), "ne\n  two\nthree\n");
    }

    /// Writes `contents` to a file in `dir` and returns its path.
    fn file_in(dir: &tempfile::TempDir, name: &str, contents: &[u8]) -> std::path::PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn read_inserts_below_the_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir, "ins.txt", b"  p\n\tq\n  r\n");
        let mut ed = Editor::from_text("a\n  b\nc");
        ed.feed(&format!(":2r {}<CR>", path.display()));
        assert_eq!(ed.buf().lines(), ["a", "  b", "  p", "\tq", "  r", "c"]);
        // The cursor goes to the last new line, on its first non-blank.
        assert_eq!((ed.cursor().line, ed.cursor().col), (4, 2));
        assert!(ed.message().unwrap().text.ends_with("3L, 11B"));
        assert!(!ed.message().unwrap().error);
    }

    #[test]
    fn read_line_zero_inserts_above_the_first_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir, "top.txt", b"x\ny\n");
        let mut ed = Editor::from_text("a\nb");
        ed.feed(&format!(":0r {}<CR>", path.display()));
        assert_eq!(ed.buf().lines(), ["x", "y", "a", "b"]);
        assert_eq!((ed.cursor().line, ed.cursor().col), (1, 0));
        ed.feed(":1r<CR>");
        assert!(ed.message().unwrap().text.starts_with("E32"));
    }

    #[test]
    fn read_with_no_range_uses_the_cursor_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir, "cur.txt", b"new\n");
        let mut ed = Editor::from_text("a\nb\nc");
        ed.feed("j");
        ed.feed(&format!(":r {}<CR>", path.display()));
        assert_eq!(ed.buf().lines(), ["a", "b", "new", "c"]);
        assert_eq!(ed.cursor().line, 2);
    }

    #[test]
    fn read_empty_file_leaves_lines_and_cursor_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir, "empty.txt", b"");
        let mut ed = Editor::from_text("a\n  b\nc");
        ed.feed(&format!(":2r {}<CR>", path.display()));
        assert_eq!(ed.buf().lines(), ["a", "  b", "c"]);
        assert_eq!((ed.cursor().line, ed.cursor().col), (1, 2));
    }

    #[test]
    fn read_into_empty_buffer_keeps_its_first_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir, "into.txt", b"one\ntwo");
        let mut ed = Editor::new();
        ed.feed(&format!(":r {}<CR>", path.display()));
        assert_eq!(ed.buf().lines(), ["", "one", "two"]);
        assert_eq!(ed.cursor().line, 2);
    }

    #[test]
    fn read_decodes_like_load() {
        let dir = tempfile::tempdir().unwrap();
        // CRLF line ends are removed, as `:e` does.
        let crlf = file_in(&dir, "crlf.txt", b"a\r\nb\r\n");
        let mut ed = Editor::from_text("x");
        ed.feed(&format!(":r {}<CR>", crlf.display()));
        assert_eq!(ed.buf().lines(), ["x", "a", "b"]);
        // No final newline: the last line still counts.
        let noeol = file_in(&dir, "noeol.txt", b"c\nd");
        ed.feed(&format!(":r {}<CR>", noeol.display()));
        assert_eq!(ed.buf().lines(), ["x", "a", "b", "c", "d"]);
    }

    #[test]
    fn read_is_one_undo_step() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir, "undo.txt", b"1\n2\n3\n");
        let mut ed = Editor::from_text("a\nb");
        ed.feed(&format!(":1r {}<CR>", path.display()));
        assert_eq!(ed.buf().line_count(), 5);
        ed.feed("u");
        assert_eq!(ed.buf().lines(), ["a", "b"]);
        ed.feed("<C-r>");
        assert_eq!(ed.buf().line_count(), 5);
    }

    #[test]
    fn read_errors_leave_the_buffer_alone() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.txt");
        let mut ed = Editor::from_text("a\nb");
        ed.feed(&format!(":2r {}<CR>", missing.display()));
        assert!(ed.message().unwrap().text.starts_with("E484"));
        assert!(ed.message().unwrap().error);
        assert_eq!(ed.buf().lines(), ["a", "b"]);
        assert!(!ed.buf().modified);
        // A directory cannot be read either.
        ed.feed(&format!(":r {}<CR>", dir.path().display()));
        assert!(ed.message().unwrap().text.starts_with("E484"));
        assert_eq!(ed.buf().lines(), ["a", "b"]);
        ed.feed(":9r missing.txt<CR>");
        assert!(ed.message().unwrap().text.starts_with("E16"));
        ed.feed(":r!ls<CR>");
        assert!(ed.message().unwrap().text.contains("not supported"));
        assert_eq!(ed.buf().lines(), ["a", "b"]);
    }

    #[test]
    fn read_without_a_name_reads_the_current_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = file_in(&dir, "self.txt", b"s\n");
        let mut ed = Editor::new();
        ed.feed(&format!(":e {}<CR>", path.display()));
        ed.feed(":r<CR>");
        assert_eq!(ed.buf().lines(), ["s", "s"]);
    }

    // `:checktime`, with real files.

    use crate::motion::Pos;
    use crate::reload::{DiskState, Stale};
    use crate::undo::UndoSource;
    use std::time::{Duration, SystemTime};

    /// A file `f.txt` in a new temp dir, opened in an editor.
    fn open_file(dir: &tempfile::TempDir, contents: &str) -> (Editor, std::path::PathBuf) {
        let path = dir.path().join("f.txt");
        fs::write(&path, contents).unwrap();
        (Editor::open(&path), path)
    }

    fn message_of(ed: &Editor) -> String {
        ed.message().map(|m| m.text.clone()).unwrap_or_default()
    }

    #[test]
    fn checktime_with_no_change_says_nothing_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, _) = open_file(&dir, "one\ntwo\n");
        ed.feed("jl");
        let seq = ed.doc().undo.seq();
        ed.feed(":checktime<CR>");
        assert_eq!(message_of(&ed), "", "no message");
        assert_eq!(ed.buf().lines(), ["one", "two"]);
        assert_eq!(ed.cursor(), Pos::new(1, 1));
        assert_eq!(ed.doc().undo.seq(), seq, "no undo step");
        assert!(!ed.buf().modified);
        assert_eq!(ed.doc().stale, None);
    }

    #[test]
    fn echo_suppression_a_write_does_not_count_as_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\n");
        ed.feed("ix<Esc>:w<CR>");
        assert_eq!(fs::read_to_string(&path).unwrap(), "xone\n");
        // Even with autoread on, our own write is not reloaded.
        ed.feed(":set autoread<CR>");
        let seq = ed.doc().undo.seq();
        ed.feed(":checktime<CR>");
        assert!(!message_of(&ed).starts_with('W'), "{}", message_of(&ed));
        assert_eq!(ed.buf().lines(), ["xone"]);
        assert_eq!(ed.doc().undo.seq(), seq);
        assert!(!ed.buf().modified);
        assert_eq!(ed.doc().stale, None);
    }

    #[test]
    fn a_change_after_our_write_is_still_seen() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\n");
        ed.feed("ix<Esc>:w<CR>");
        fs::write(&path, "other\n").unwrap();
        ed.feed(":checktime<CR>");
        assert!(message_of(&ed).starts_with("W11"), "{}", message_of(&ed));
        assert_eq!(ed.buf().lines(), ["xone"]);
    }

    #[test]
    fn changed_with_autoread_reloads_as_one_external_undo_step() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\ntwo\nthree\n");
        ed.feed("jj:set autoread<CR>");
        let seq = ed.doc().undo.seq();
        fs::write(&path, "one\nTWO\nthree\nfour\n").unwrap();
        ed.feed(":checktime<CR>");
        assert_eq!(ed.buf().lines(), ["one", "TWO", "three", "four"]);
        assert!(!ed.buf().modified);
        assert_eq!(ed.doc().undo.seq(), seq + 1);
        assert_eq!(ed.doc().undo.source(seq + 1), Some(UndoSource::External));
        assert!(
            message_of(&ed).contains("changed on disk"),
            "{}",
            message_of(&ed)
        );
        assert_eq!(ed.doc().stale, None);
        // `u` goes back in one step.
        ed.feed("u");
        assert_eq!(ed.buf().lines(), ["one", "two", "three"]);
        ed.feed("<C-r>");
        assert_eq!(ed.buf().lines(), ["one", "TWO", "three", "four"]);
        // And the disk state now is the new file: a second check does nothing.
        let seq = ed.doc().undo.seq();
        ed.feed(":checktime<CR>");
        assert_eq!(ed.doc().undo.seq(), seq);
    }

    #[test]
    fn changed_without_autoread_is_reported_and_the_buffer_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\n");
        fs::write(&path, "two\n").unwrap();
        ed.feed(":checktime<CR>");
        let want = format!(
            "W11: Warning: File \"{}\" has changed since editing started",
            path.display()
        );
        assert_eq!(message_of(&ed), want);
        assert_eq!(ed.buf().lines(), ["one"]);
        assert!(!ed.buf().modified);
        assert_eq!(ed.doc().stale, Some(Stale::Changed));
        // Still reported by the next check: the disk state was not taken.
        ed.feed(":checktime<CR>");
        assert!(message_of(&ed).starts_with("W11"));
    }

    #[test]
    fn changed_and_modified_is_w12_and_the_buffer_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\n");
        ed.feed("ix<Esc>:set autoread<CR>");
        fs::write(&path, "two\n").unwrap();
        ed.feed(":checktime<CR>");
        assert!(message_of(&ed).starts_with("W12"), "{}", message_of(&ed));
        assert_eq!(ed.buf().lines(), ["xone"]);
        assert!(ed.buf().modified);
        assert_eq!(ed.doc().stale, Some(Stale::Changed));
    }

    #[test]
    fn deleted_file_is_e211_and_the_buffer_kept() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\n");
        fs::remove_file(&path).unwrap();
        ed.feed(":checktime<CR>");
        assert!(message_of(&ed).starts_with("E211"), "{}", message_of(&ed));
        assert_eq!(ed.buf().lines(), ["one"]);
        assert_eq!(ed.doc().stale, Some(Stale::Deleted));
    }

    #[test]
    fn a_file_created_after_editing_started_is_w13() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let mut ed = Editor::open(&path);
        ed.feed(":checktime<CR>");
        assert!(
            !message_of(&ed).starts_with('W'),
            "nothing before the file exists"
        );
        fs::write(&path, "made\n").unwrap();
        ed.feed(":checktime<CR>");
        assert!(message_of(&ed).starts_with("W13"), "{}", message_of(&ed));
        assert_eq!(ed.buf().lines(), [""]);
    }

    #[test]
    fn a_touched_file_with_the_same_contents_is_not_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\n");
        ed.feed(":set autoread<CR>");
        let later = SystemTime::now() + Duration::from_secs(3600);
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        let seq = ed.doc().undo.seq();
        ed.feed(":checktime<CR>");
        assert_eq!(ed.doc().undo.seq(), seq);
        assert_eq!(ed.buf().lines(), ["one"]);
        assert_eq!(message_of(&ed), "");
        // The recorded modification time is now the file's.
        let mtime = fs::metadata(&path).unwrap().modified().ok();
        assert_eq!(ed.doc().disk.as_ref().unwrap().mtime, mtime);
    }

    #[test]
    fn disk_state_is_the_hash_and_mtime_of_what_was_read_and_written() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\r\ntwo\r\n");
        let read = fs::read(&path).unwrap();
        let mtime = fs::metadata(&path).unwrap().modified().ok();
        assert_eq!(ed.doc().disk, Some(DiskState::of(mtime, &read)));
        assert_eq!(
            ed.doc().disk.as_ref().unwrap().hash,
            crate::reload::fnv1a(&read)
        );
        // A write records the bytes written, with their line ends.
        ed.feed("ix<Esc>:w<CR>");
        let written = fs::read(&path).unwrap();
        assert_eq!(written, b"xone\r\ntwo\r\n");
        let mtime = fs::metadata(&path).unwrap().modified().ok();
        assert_eq!(ed.doc().disk, Some(DiskState::of(mtime, &written)));
        // A buffer with no file has no disk state.
        assert_eq!(Editor::from_text("x").doc().disk, None);
    }

    #[test]
    fn checktime_argument_is_a_buffer_address_as_in_vim() {
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\n");
        ed.feed(":set autoread<CR>");
        // Buffer 1 is the current buffer: it is checked.
        fs::write(&path, "two\n").unwrap();
        ed.feed(":checktime 1<CR>");
        assert_eq!(ed.buf().lines(), ["two"]);
        assert!(message_of(&ed).contains("changed on disk"));
        // Another buffer number is silently skipped (Vim does the same).
        fs::write(&path, "three\n").unwrap();
        ed.feed(":checktime 2<CR>");
        assert_eq!(ed.buf().lines(), ["two"]);
        assert_eq!(message_of(&ed), "");
        // The current buffer by `%` or by a part of its name.
        ed.feed(":checktime %<CR>");
        assert_eq!(ed.buf().lines(), ["three"]);
        fs::write(&path, "four\n").unwrap();
        ed.feed(":checktime f.tx<CR>");
        assert_eq!(ed.buf().lines(), ["four"]);
        // Zero is E939; a name that matches nothing is E94.
        ed.feed(":checktime 0<CR>");
        assert!(message_of(&ed).starts_with("E939"), "{}", message_of(&ed));
        ed.feed(":checktime nosuch<CR>");
        assert_eq!(message_of(&ed), "E94: No matching buffer for nosuch");
        ed.feed(":checktime 1,2<CR>");
        assert!(message_of(&ed).starts_with("E94"), "{}", message_of(&ed));
    }

    #[test]
    fn autoread_uses_the_effective_global_local_value() {
        // 'autoread' is global-local: a local value set with `:setlocal` wins
        // over the global one, and an unset local value means the global one.
        let dir = tempfile::tempdir().unwrap();
        let (mut ed, path) = open_file(&dir, "one\n");
        fs::write(&path, "two\n").unwrap();
        ed.feed(":checktime<CR>");
        assert!(message_of(&ed).starts_with("W11"), "{}", message_of(&ed));
        ed.feed(":setlocal autoread<CR>:checktime<CR>");
        assert_eq!(ed.buf().lines(), ["two"]);
        assert!(!ed.buf().modified);
        // Set the global value only: the local one is still unset, so it counts.
        let dir2 = tempfile::tempdir().unwrap();
        let (mut ed2, path2) = open_file(&dir2, "one\n");
        ed2.feed(":setglobal autoread<CR>");
        fs::write(&path2, "two\n").unwrap();
        ed2.feed(":checktime<CR>");
        assert_eq!(ed2.buf().lines(), ["two"]);
    }

    #[test]
    fn checktime_name_match_without_a_file_is_e94() {
        let mut ed = Editor::from_text("x");
        ed.feed(":checktime f<CR>");
        assert_eq!(message_of(&ed), "E94: No matching buffer for f");
    }
}

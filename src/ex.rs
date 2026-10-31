//! Ex commands typed after `:`: line ranges (numbers, `.`, `$`, `%`, `'<`,
//! `/pat/`, offsets, `,` and `;`), files (`:w :q :wq :x :e`), line commands
//! (`:d :y :pu :m :t :j :> :<`), `:s`, `:g`, `:normal`, `:undo`, `:set`.

use crate::editor::{Editor, Mode, describe};
use crate::key::Key;
use crate::motion::{Pos, first_non_blank};
use crate::normal::Op;
use crate::ops::OpArg;
use crate::regex::Regex;
use crate::search::{SearchFlags, Start, searchit};
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

impl Editor {
    pub(crate) fn execute_ex(&mut self, cmd: &str) {
        // `|` separates commands, except after :g and :normal.
        let mut rest = cmd.to_string();
        loop {
            let (this, next) = self.split_bar(&rest);
            if let Err(msg) = self.execute_one(&this) {
                return self.error(msg);
            }
            match next {
                Some(n) => rest = n,
                None => break,
            }
            if self.quit {
                break;
            }
        }
    }

    /// Splits `cmd` at the first `|` that separates commands.
    fn split_bar(&self, cmd: &str) -> (String, Option<String>) {
        let t = cmd.trim_start_matches([' ', '\t', ':']);
        // Skip the range to find the command name.
        let name: String = {
            let mut s = t;
            // A crude skip over addresses: find first alphabetic char not in
            // a pattern. Good enough to recognize :g and :normal.
            let mut depth = false;
            let mut end = 0;
            for (i, c) in s.char_indices() {
                if c == '/' || c == '?' {
                    depth = !depth;
                }
                if !depth && c.is_ascii_alphabetic() {
                    end = i;
                    break;
                }
            }
            s = &s[end..];
            s.chars().take_while(|c| c.is_ascii_alphabetic()).collect()
        };
        let takes_bar =
            abbrev(&name, "global", 1) || abbrev(&name, "vglobal", 1) || abbrev(&name, "normal", 4);
        if takes_bar {
            return (cmd.to_string(), None);
        }
        let chars: Vec<char> = cmd.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' && chars.get(i + 1) == Some(&'|') {
                i += 2;
                continue;
            }
            if chars[i] == '|' {
                let a: String = chars[..i].iter().collect();
                let b: String = chars[i + 1..].iter().collect();
                return (a, Some(b));
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
                let l = range.line2.clamp(1, self.buf.line_count());
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
            if self.buf.modified || !p.arg.is_empty() {
                self.cmd_write(&p, true);
            } else {
                self.quit = true;
            }
        } else if abbrev(n, "quit", 1) || abbrev(n, "qall", 2) {
            if self.buf.modified && !p.bang {
                return Err("E37: No write since last change (add ! to override)".into());
            }
            self.quit = true;
        } else if abbrev(n, "version", 2) {
            self.info(crate::VERSION_MESSAGE);
        } else if abbrev(n, "edit", 1) {
            self.cmd_edit(&p);
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
        } else if abbrev(n, "undo", 1) {
            self.undo_cmd(1, false);
        } else if abbrev(n, "redo", 3) {
            self.undo_cmd(1, true);
        } else if abbrev(n, "print", 1)
            || abbrev(n, "number", 2)
            || n == "#"
            || abbrev(n, "list", 1)
        {
            let l = range.line2.clamp(1, self.buf.line_count());
            self.goto_line_first_non_blank(l - 1);
            let text = self.buf.line_string(l - 1);
            self.info(text);
        } else if n == "=" {
            let l = if range.count == 0 {
                self.buf.line_count()
            } else {
                range.line2
            };
            self.info(l.to_string());
        } else if abbrev(n, "set", 2) || abbrev(n, "setlocal", 6) {
            self.ex_set(arg_trim)?;
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
        let cur = self.cursor.line + 1;
        let last = self.buf.line_count();
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
                        self.cursor.line = base - 1;
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
        match self.last_visual {
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
        let last = self.buf.line_count();
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
        match self.last_visual {
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
            pat.to_string()
        };
        let re = Regex::new(&pattern, &self.re_opts())?;
        let line = base.clamp(1, self.buf.line_count()) - 1;
        let start = if fwd {
            Pos::new(line, usize::MAX / 4)
        } else {
            Pos::new(line, 0)
        };
        let fl = SearchFlags {
            start: false,
            end: false,
            wrapscan: self.opts.wrapscan,
        };
        match searchit(&self.buf, &re, Start::At(start), fwd, 1, &fl) {
            Some(f) => Ok(f.pos.line + 1),
            None => Err(format!("E486: Pattern not found: {pattern}")),
        }
    }

    // ---- files ----------------------------------------------------------

    fn cmd_write(&mut self, p: &Parsed, then_quit: bool) {
        let target: PathBuf = if p.arg.is_empty() {
            match &self.buf.path {
                Some(path) => path.clone(),
                None => return self.error("E32: No file name"),
            }
        } else {
            PathBuf::from(p.arg)
        };
        let own_file = self.buf.path.as_deref() == Some(target.as_path());
        if !own_file && !p.bang && target.exists() {
            return self.error("E13: File exists (add ! to override)");
        }
        let is_new = !target.exists();
        match self.buf.write_to(&target) {
            Ok(stats) => {
                if self.buf.path.is_none() {
                    self.buf.path = Some(target.clone());
                }
                if self.buf.path.as_deref() == Some(target.as_path()) {
                    self.mark_saved();
                }
                let new = if is_new { " [New]" } else { "" };
                self.info(format!(
                    "\"{}\"{new} {} written",
                    target.display(),
                    describe(stats, self.buf.crlf)
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
        if self.buf.modified && !p.bang {
            return self.error("E37: No write since last change (add ! to override)");
        }
        let path = if p.arg.is_empty() {
            match &self.buf.path {
                Some(path) => path.clone(),
                None => return self.error("E32: No file name"),
            }
        } else {
            PathBuf::from(p.arg)
        };
        self.edit_file(Path::new(&path));
    }

    // ---- line commands --------------------------------------------------

    fn check_range_lines(&self, r: Range) -> ExRes<()> {
        if r.line1 < 1 || r.line2 > self.buf.line_count() || r.line1 > r.line2 {
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
                r.line2 = (r.line2 + n - 1).min(self.buf.line_count());
            } else {
                return Err(format!("E488: Trailing characters: {rest}"));
            }
        }
        if r.line1 == 0 {
            r.line1 = 1;
        }
        self.check_range_lines(r)?;
        if self.buf.has_no_lines() && delete {
            return Ok(());
        }
        let saved = self.cursor;
        self.cursor = Pos::new(r.line1 - 1, 0);
        self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
        let mut oa = self.line_oparg(if delete { Op::Delete } else { Op::Yank }, r, reg);
        if delete {
            self.op_delete(&mut oa);
            self.keep_curswant = false;
        } else {
            self.op_yank(&oa, false, true);
            // The cursor does not move for :yank.
            self.cursor = saved;
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
            self.buf.insert_line(at + i, l);
        }
        self.cursor.line = at + n - 1;
        self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
        self.uline = None;
        if n > 2 {
            self.info(format!("{n} more lines"));
        }
        let _ = reg.kind;
        Ok(())
    }

    /// Parses the destination of `:m` / `:t`.
    fn copymove_dest(&mut self, arg: &str) -> ExRes<usize> {
        let (addr, rest) = self.get_address(arg, self.cursor.line + 1)?;
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
                self.cursor.line = (if dest >= r.line2 { dest } else { r.line2 }) - 1;
                self.cursor.line = r.line2 - 1;
                self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
                return Ok(());
            }
            let moved: Vec<String> = (r.line1 - 1..r.line2)
                .map(|l| self.buf.line_string(l))
                .collect();
            self.u_save(0, self.buf.line_count(), false);
            if dest >= r.line2 {
                for (i, l) in moved.iter().enumerate() {
                    self.buf.insert_line(dest + i, l);
                }
                self.buf.delete_lines(r.line1 - 1, n);
                self.cursor.line = dest - 1;
            } else {
                self.buf.delete_lines(r.line1 - 1, n);
                for (i, l) in moved.iter().enumerate() {
                    self.buf.insert_line(dest + i, l);
                }
                self.cursor.line = dest + n - 1;
            }
            if n > 2 {
                self.info(format!("{n} lines moved"));
            }
        } else {
            let copied: Vec<String> = (r.line1 - 1..r.line2)
                .map(|l| self.buf.line_string(l))
                .collect();
            self.u_save(dest, 0, false);
            for (i, l) in copied.iter().enumerate() {
                self.buf.insert_line(dest + i, l);
            }
            self.cursor.line = dest + n - 1;
            if n > 2 {
                self.info(format!("{n} more lines"));
            }
        }
        self.uline = None;
        self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
        Ok(())
    }

    fn ex_join(&mut self, mut r: Range, bang: bool, arg: &str) -> ExRes<()> {
        self.cursor.line = (r.line1 - 1).min(self.buf.last_line());
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
            if r.line2 == self.buf.line_count() {
                self.beep();
                return Ok(());
            }
            r.line2 += 1;
        }
        r.line2 = r.line2.min(self.buf.line_count());
        if r.line1 == r.line2 {
            return Ok(());
        }
        self.cursor = Pos::new(r.line1 - 1, 0);
        self.do_join(r.line2 - r.line1 + 1, !bang, true);
        self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
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
            r.line2 = (r.line2 + n - 1).min(self.buf.line_count());
        } else if !rest.is_empty() {
            return Err(format!("E488: Trailing characters: {rest}"));
        }
        self.check_range_lines(r)?;
        self.cursor = Pos::new(r.line1 - 1, 0);
        self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
        let oa = self.line_oparg(if left { Op::LShift } else { Op::RShift }, r, None);
        self.op_shift(&oa, false, amount);
        self.cursor.line = r.line2 - 1;
        self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
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
                self.cursor = Pos::new((l - 1).min(self.buf.last_line()), 0);
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
            (1, self.buf.line_count())
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
            pat
        };
        let re = Regex::new(&pattern, &self.re_opts())?;
        let marked: Vec<usize> = (l1 - 1..l2)
            .filter(|&l| re.is_match(&self.buf, l) != invert)
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
        let old_lines = self.buf.line_count() as i64;
        self.buf.tracked = marked;
        self.global_busy = true;
        self.global_need_beginline = false;
        let n = self.buf.tracked.len();
        for i in 0..n {
            let l = self.buf.tracked[i];
            if l == usize::MAX {
                continue;
            }
            self.cursor = Pos::new(l.min(self.buf.last_line()), 0);
            self.execute_ex(cmd);
            if self.quit {
                break;
            }
        }
        self.global_busy = false;
        self.buf.tracked.clear();
        if self.global_need_beginline {
            self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
        } else {
            let len = self.buf.line_len(self.cursor.line);
            if self.cursor.col >= len {
                self.cursor.col = len.saturating_sub(1);
            }
        }
        let diff = self.buf.line_count() as i64 - old_lines;
        if diff.abs() > 2 {
            let what = if diff > 0 {
                "more lines"
            } else {
                "fewer lines"
            };
            self.info(format!("{} {what}", diff.abs()));
        }
        Ok(())
    }

    // ---- :set -----------------------------------------------------------

    fn ex_set(&mut self, arg: &str) -> ExRes<()> {
        for item in arg.split_whitespace() {
            let (name, val): (&str, Option<&str>) = match item.split_once(['=', ':']) {
                Some((n, v)) => (n, Some(v)),
                None => (item, None),
            };
            let (base, on) = if let Some(b) = name.strip_prefix("no") {
                (b, false)
            } else if let Some(b) = name.strip_prefix("inv") {
                (b, !self.get_bool_opt(b))
            } else {
                (name.trim_end_matches(['!', '?', '&']), true)
            };
            let toggle = name.ends_with('!');
            let base = base.trim_end_matches(['!', '?']);
            let on = if toggle { !self.get_bool_opt(base) } else { on };
            match base {
                "ic" | "ignorecase" => self.opts.ignorecase = on,
                "scs" | "smartcase" => self.opts.smartcase = on,
                "ws" | "wrapscan" => self.opts.wrapscan = on,
                "hls" | "hlsearch" | "is" | "incsearch" | "nu" | "number" | "list" | "wrap"
                | "ru" | "ruler" | "sc" | "showcmd" | "magic" | "et" | "expandtab" | "ai"
                | "autoindent" | "sw" | "shiftwidth" | "ts" | "tabstop" | "sol" | "startofline"
                | "js" | "joinspaces" | "hidden" | "hid" => {
                    let _ = val;
                }
                _ => return Err(format!("E518: Unknown option: {base}")),
            }
        }
        Ok(())
    }

    fn get_bool_opt(&self, name: &str) -> bool {
        match name {
            "ic" | "ignorecase" => self.opts.ignorecase,
            "scs" | "smartcase" => self.opts.smartcase,
            "ws" | "wrapscan" => self.opts.wrapscan,
            _ => false,
        }
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
                rest = "";
                return self.do_substitute(r, &pattern, &rep, SubFlags::default(), rest);
            };
            pattern = self.pick_pattern(&pat)?;
            let (mut rp, after2) = split_rep(after, delim);
            // `~` stands for the previous replacement.
            rp = expand_tilde(&rp, &self.last_sub_raw);
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
                Some('c') => {}
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
            r.line2 = (r.line2 + cnt - 1).min(self.buf.line_count());
        }
        self.do_substitute(r, &pattern, &rep, flags, "")
    }

    fn pick_pattern(&mut self, pat: &str) -> ExRes<String> {
        if pat.is_empty() {
            self.last_pat
                .clone()
                .ok_or_else(|| "E35: No previous regular expression".to_string())
        } else {
            Ok(pat.to_string())
        }
    }

    fn do_substitute(
        &mut self,
        r: Range,
        pattern: &str,
        rep: &str,
        flags: SubFlags,
        _rest: &str,
    ) -> ExRes<()> {
        self.check_range_lines(r)?;
        self.last_pat = Some(pattern.to_string());
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

        let mut line2 = r.line2;
        let mut lnum = r.line1 - 1;
        let mut nsubs = 0usize;
        let mut nlines = 0usize;
        let mut last_line: Option<usize> = None;
        let multiline_pat = re_multiline(pattern);
        let saved_cursor = self.cursor;
        while lnum < line2 && lnum < self.buf.line_count() {
            let Some(mut m) = re.match_line(&self.buf, lnum, 0) else {
                lnum += 1;
                continue;
            };
            // A line break before \zs: the match starts in a later line.
            if m.start.0 > lnum {
                lnum = m.start.0;
                if lnum >= line2 {
                    break;
                }
            }
            let mut sub_line: Vec<char> = self.line_vec(lnum);
            let mut new_start: Option<Vec<char>> = None;
            let mut out_lines: Vec<String> = Vec::new();
            let mut copycol = 0usize;
            let mut matchcol = 0usize;
            let mut prev_matchcol: i64 = -1;
            let mut did_sub = false;
            let mut skip_match = false;
            loop {
                let mut do_again = false;
                let mut nmatch = m.end.0 - m.start.0 + 1;
                // 1. An empty match right after the previous match does not
                // count (the strange vi behaviour; also avoids endless loops).
                let dup =
                    matchcol as i64 == prev_matchcol && m.end.0 == m.start.0 && matchcol == m.end.1;
                if dup {
                    if matchcol >= sub_line.len() {
                        skip_match = true;
                    } else {
                        matchcol += 1;
                    }
                } else {
                    matchcol = m.end.1;
                    prev_matchcol = matchcol as i64;
                    // Like Vim, the cursor sits at the start of the line (this
                    // is also the cursor undo returns to).
                    self.cursor = Pos::new(lnum, 0);
                    if flags.count_only {
                        if nmatch > 1 {
                            matchcol = sub_line.len();
                            nmatch = 1;
                            skip_match = true;
                        }
                        nsubs += 1;
                        did_sub = true;
                    } else {
                        // 3. substitute the string.
                        let out = self.expand_replacement(&rep_chars, &m);
                        let ns = new_start.get_or_insert_with(Vec::new);
                        let sc = m.start.1.min(sub_line.len());
                        if sc >= copycol {
                            ns.extend(&sub_line[copycol..sc]);
                        }
                        for piece in out {
                            match piece {
                                RepPiece::Text(c) => ns.push(c),
                                RepPiece::Break => {
                                    out_lines.push(ns.iter().collect());
                                    ns.clear();
                                }
                            }
                        }
                        nsubs += 1;
                        did_sub = true;
                        last_line = Some(lnum + out_lines.len());
                        if nmatch > 1 {
                            // The match joined lines: replace them now and
                            // continue in the joined line.
                            let last = self.buf.line_count() - 1;
                            let end_l = m.end.0.min(last);
                            if m.end.0 > last {
                                skip_match = true;
                            }
                            let tail: Vec<char> = if m.end.0 <= last {
                                self.line_vec(m.end.0)
                            } else {
                                Vec::new()
                            };
                            let mut produced = new_start.take().unwrap_or_default();
                            let plen = produced.len();
                            produced.extend(&tail[m.end.1.min(tail.len())..]);
                            let n_old = end_l - lnum + 1;
                            let mut new_lines = std::mem::take(&mut out_lines);
                            new_lines.push(produced.iter().collect());
                            self.u_save(lnum, n_old, false);
                            // Insert the new lines first so that the buffer
                            // never becomes empty.
                            for (i, l) in new_lines.iter().enumerate() {
                                self.buf.insert_line(lnum + i, l);
                            }
                            self.buf.delete_lines(lnum + new_lines.len(), n_old);
                            let added = new_lines.len();
                            line2 = (line2 + added).saturating_sub(n_old);
                            lnum += added - 1;
                            last_line = Some(lnum);
                            sub_line = self.line_vec(lnum);
                            copycol = 0;
                            matchcol = plen;
                            prev_matchcol = plen as i64;
                            if lnum < line2 {
                                do_again = true;
                            }
                        } else {
                            copycol = m.end.1;
                        }
                    }
                }
                // skip:
                let lastone = skip_match
                    || !(flags.global || do_again)
                    || (matchcol >= sub_line.len() && nmatch <= 1 && !multiline_pat);
                if lastone {
                    break;
                }
                match re.match_line(&self.buf, lnum, matchcol) {
                    Some(n) if n.start.0 == lnum => m = n,
                    _ => break,
                }
            }
            if did_sub {
                nlines += 1;
                if !flags.count_only {
                    // Flush the line (the text after the last match is kept).
                    if let Some(mut ns) = new_start.take() {
                        if copycol <= sub_line.len() {
                            ns.extend(&sub_line[copycol..]);
                        }
                        self.u_save(lnum, 1, false);
                        let added = out_lines.len();
                        self.buf.set_line(lnum, &ns.iter().collect::<String>());
                        for (i, l) in out_lines.iter().enumerate() {
                            self.buf.insert_line(lnum + i, l);
                        }
                        lnum += added;
                        line2 += added;
                        last_line = Some(lnum);
                    }
                }
            }
            lnum += 1;
        }
        let did_sub_any = nsubs > 0;
        if flags.count_only {
            self.cursor = saved_cursor;
            if nsubs > 0 {
                self.info(format!(
                    "{nsubs} match{} on {nlines} line{}",
                    if nsubs == 1 { "" } else { "es" },
                    if nlines == 1 { "" } else { "s" }
                ));
            } else if !flags.no_error {
                return Err(format!("E486: Pattern not found: {pattern}"));
            }
            return Ok(());
        }
        if did_sub_any {
            if let Some(l) = last_line {
                self.cursor.line = l.min(self.buf.last_line());
                self.cursor.col = 0;
            }
            self.uline = None;
            if self.global_busy {
                self.global_need_beginline = true;
            } else {
                self.cursor.col = first_non_blank(&self.buf, self.cursor.line, true);
            }
            if nsubs > 2 {
                self.info(format!(
                    "{nsubs} substitution{} on {nlines} line{}",
                    if nsubs == 1 { "" } else { "s" },
                    if nlines == 1 { "" } else { "s" }
                ));
            }
        } else if !self.global_busy && !flags.no_error {
            return Err(format!("E486: Pattern not found: {pattern}"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SubFlags {
    pub global: bool,
    pub ic: Option<bool>,
    pub no_error: bool,
    pub count_only: bool,
    pub print: bool,
    pub use_search: bool,
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
            | "move"
            | "mo"
            | "mov"
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
            | "setl"
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
        if l < self.buf.line_count() {
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
        assert_eq!(ed.buf.line_count(), 3);
        assert!(ed.message().unwrap().text.ends_with("3L, 16B"));
        ed.feed(":2<CR>");
        assert_eq!((ed.cursor().line, ed.cursor().col), (1, 2));
        ed.feed(":$<CR>");
        assert_eq!(ed.cursor().line, 2);
        ed.feed(":.-2<CR>");
        assert_eq!(ed.cursor().line, 0);
        ed.feed(":ve<CR>");
        assert!(
            ed.message()
                .unwrap()
                .text
                .contains("modified version of Vim")
        );
        ed.feed(":frob<CR>");
        assert!(ed.message().unwrap().text.starts_with("E492"));
        ed.feed("x:x<CR>");
        assert!(ed.should_quit());
        assert_eq!(fs::read_to_string(&path).unwrap(), "ne\n  two\nthree\n");
    }
}

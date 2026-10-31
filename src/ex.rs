//! Ex commands typed after `:`. Milestone 1 supports line addresses
//! (`:12`, `:$`, `:.+3`), `:w`, `:q`, `:wq`, `:x`, `:e` and their `!` forms.

use crate::editor::{Editor, describe};
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

impl Editor {
    pub(crate) fn execute_ex(&mut self, cmd: &str) {
        let cmd = cmd.trim_start_matches([' ', '\t', ':']).trim_end();
        if cmd.is_empty() {
            return;
        }
        let (addr, rest) = match self.parse_address(cmd) {
            Ok(v) => v,
            Err(msg) => return self.error(msg),
        };
        let rest = rest.trim_start();
        if rest.is_empty() {
            if let Some(line) = addr {
                self.goto_line_first_non_blank(line.saturating_sub(1));
            }
            return;
        }
        let name_len = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let (name, after) = rest.split_at(name_len);
        let (bang, after) = match after.strip_prefix('!') {
            Some(a) => (true, a),
            None => (false, after),
        };
        let p = Parsed {
            name,
            bang,
            arg: after.trim(),
        };
        if name.is_empty() {
            return self.error(format!("E492: Not an editor command: {cmd}"));
        }
        if abbrev(p.name, "write", 1) {
            self.cmd_write(&p, false);
        } else if p.name == "wq" {
            self.cmd_write(&p, true);
        } else if abbrev(p.name, "xit", 1) || abbrev(p.name, "exit", 3) {
            if self.buf.modified || !p.arg.is_empty() {
                self.cmd_write(&p, true);
            } else {
                self.quit = true;
            }
        } else if abbrev(p.name, "quit", 1) || abbrev(p.name, "qall", 2) {
            if self.buf.modified && !p.bang {
                self.error("E37: No write since last change (add ! to override)");
            } else {
                self.quit = true;
            }
        } else if abbrev(p.name, "version", 2) {
            self.info(crate::VERSION_MESSAGE);
        } else if abbrev(p.name, "edit", 1) {
            self.cmd_edit(&p);
        } else {
            self.error(format!("E492: Not an editor command: {cmd}"));
        }
    }

    /// Parses an optional address: a number, `.` or `$`, followed by any
    /// `+N`/`-N` offsets. Returns the 1-based line.
    fn parse_address<'a>(&self, s: &'a str) -> Result<(Option<usize>, &'a str), String> {
        let mut rest = s;
        let cur = self.cursor.line as i64 + 1;
        let last = self.buf.line_count() as i64;
        let take_num = |r: &'a str| -> (Option<i64>, &'a str) {
            let n = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
            if n == 0 {
                (None, r)
            } else {
                (Some(r[..n].parse().unwrap_or(i64::MAX)), &r[n..])
            }
        };
        let mut line = match rest.chars().next() {
            Some('.') => {
                rest = &rest[1..];
                Some(cur)
            }
            Some('$') => {
                rest = &rest[1..];
                Some(last)
            }
            _ => {
                let (n, r) = take_num(rest);
                rest = r;
                n
            }
        };
        while let Some(sign @ ('+' | '-')) = rest.chars().next() {
            let (n, r) = take_num(&rest[1..]);
            rest = r;
            let delta = n.unwrap_or(1);
            let base = line.unwrap_or(cur);
            line = Some(if sign == '+' {
                base.saturating_add(delta)
            } else {
                base - delta
            });
        }
        match line {
            None => Ok((None, rest)),
            Some(l) if l < 0 || l > last => Err("E16: Invalid range".into()),
            Some(l) => Ok((Some(l as usize), rest)),
        }
    }

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
                    self.buf.modified = false;
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

// SPDX-License-Identifier: Apache-2.0
//! The marks that are not lowercase, and the mark commands `:marks` and
//! `:delmarks`. `'[` and `']` are the last change or yank (`Document::op_start`
//! and `op_end`, Vim's `b_op_start` and `b_op_end`); `'<` and `'>` are the
//! last Visual area (`Document::last_visual`); `'"` is the cursor when the
//! buffer was last left, which is always line 1 here because rvim has one
//! buffer. The rules follow Vim's getmark(), setmark_pos(), clrallmarks(),
//! show_one_mark() and ex_delmarks() in mark.c.

use crate::editor::{Editor, LastVisual, VisualMode};
use crate::motion::Pos;

/// Vim's MAXCOL: the column of `']` after a linewise yank, and of `'>` in a
/// linewise Visual area. Read as the end of the line.
pub(crate) const MAXCOL: usize = usize::MAX;

/// The column `:marks` prints for `MAXCOL` (Vim prints the C `long`).
const MAXCOL_PRINTED: usize = 2_147_483_647;

/// Marks `:delmarks` accepts but rvim does not keep (digits from viminfo,
/// `^` and `.`): deleting them does nothing.
fn is_other_mark(c: char) -> bool {
    c.is_ascii_digit() || matches!(c, '"' | '^' | '.' | '[' | ']' | '<' | '>')
}

impl Editor {
    /// Vim's getmark() for the marks that are not lowercase: `'[`, `']`,
    /// `'<`, `'>` and `'"`. `None` when the mark is not set.
    pub(crate) fn special_mark(&self, c: char) -> Option<Pos> {
        match c {
            '[' => self.doc().op_start,
            ']' => self.doc().op_end,
            '<' | '>' => {
                let v = self.doc().last_visual?;
                // The earlier end is `'<`, whichever way the area was made.
                let p = if (c == '<') == (v.start <= v.end) {
                    v.start
                } else {
                    v.end
                };
                // A linewise area's marks are at the start and the end of
                // their lines.
                Some(match v.mode {
                    VisualMode::Line => Pos::new(p.line, if c == '<' { 0 } else { MAXCOL }),
                    _ => p,
                })
            }
            '"' => Some(Pos::default()),
            _ => None,
        }
    }

    /// `m[`, `m]`, `m<` and `m>` at the cursor (Vim's setmark_pos()). `m<`
    /// and `m>` set one end of the Visual area and keep the other.
    pub(crate) fn set_special_mark(&mut self, c: char) {
        let cursor = self.win().cursor;
        match c {
            '[' => self.doc_mut().op_start = Some(cursor),
            ']' => self.doc_mut().op_end = Some(cursor),
            '<' | '>' => {
                let mut v = self.doc().last_visual.unwrap_or(LastVisual {
                    start: cursor,
                    end: cursor,
                    mode: VisualMode::Char,
                    curswant: 0,
                });
                if c == '<' {
                    v.start = cursor;
                } else {
                    v.end = cursor;
                }
                self.doc_mut().last_visual = Some(v);
            }
            _ => {}
        }
    }

    /// `:marks [{arg}]`: lists the marks rvim has in Vim's order and format.
    /// The rows go on one message line, separated by ` | `, after Vim's
    /// header (Vim lists them one per line).
    pub(crate) fn ex_marks(&mut self, bang: bool, arg: &str) -> Result<(), String> {
        if bang {
            return Err("E477: No ! allowed".into());
        }
        let arg = arg.trim();
        self.sync_jumps();
        let wanted = |c: char| arg.is_empty() || arg.contains(c);
        let mut rows = Vec::new();
        if wanted('\'') {
            let pcmark = self.win().jumps.pcmark();
            rows.extend(self.mark_row('\'', pcmark));
        }
        // Lowercase, then file marks, as Vim's ex_marks() lists them.
        for c in ('a'..='z').chain('A'..='Z') {
            if wanted(c) {
                rows.extend(self.mark_row(c, self.buf().mark(c)));
            }
        }
        for c in ['"', '[', ']', '<', '>'] {
            if wanted(c) {
                rows.extend(self.mark_row(c, self.special_mark(c)));
            }
        }
        if rows.is_empty() {
            return Err(format!("E283: No marks matching \"{arg}\""));
        }
        self.info(format!("mark line  col file/text | {}", rows.join(" | ")));
        Ok(())
    }

    /// One row of `:marks`: `{c} {line:>6} {col:>4} {text}`, with the line
    /// text without its leading white space (Vim's show_one_mark()). The
    /// column is a byte index, as Vim's is. `None` when the mark is not set.
    fn mark_row(&self, c: char, p: Option<Pos>) -> Option<String> {
        let p = p?;
        if p.line >= self.buf().line_count() {
            return Some(format!("{c} {:>6} {:>4} -invalid-", p.line + 1, p.col));
        }
        let line = self.buf().line_string(p.line);
        let (line_no, col) = if p.col == MAXCOL {
            (p.line + 1, MAXCOL_PRINTED)
        } else {
            let len = line.chars().count();
            let before: usize = line.chars().take(p.col).map(char::len_utf8).sum();
            (p.line + 1, before + p.col.saturating_sub(len))
        };
        let text = line.trim_start_matches([' ', '\t']);
        Some(format!("{c} {line_no:>6} {col:>4} {text}"))
    }

    /// Deletes one mark (Vim's ex_delmarks() for one character). Marks
    /// rvim does not keep are accepted and ignored.
    fn delete_mark(&mut self, c: char) {
        match c {
            'a'..='z' => self.buf_mut().set_mark_unset(c),
            'A'..='Z' => self.clear_file_mark(c),
            '[' => self.doc_mut().op_start = None,
            ']' => self.doc_mut().op_end = None,
            // rvim keeps `<` and `>` as one area; deleting either drops both.
            '<' | '>' => self.doc_mut().last_visual = None,
            _ => {}
        }
    }

    /// `:delmarks!`: Vim's clrallmarks(). The previous context mark `''`
    /// and `'"` are not cleared.
    fn clear_all_marks(&mut self) {
        self.buf_mut().clear_marks();
        self.doc_mut().op_start = None;
        self.doc_mut().op_end = None;
        self.doc_mut().last_visual = None;
    }

    /// `:delmarks {marks}` and `:delmarks!`. Arguments are read as Vim does:
    /// a single mark, or a range `c-c` of marks of one kind. A bad argument
    /// is an error, and the marks before it are deleted.
    pub(crate) fn ex_delmarks(&mut self, bang: bool, arg: &str) -> Result<(), String> {
        let arg = arg.trim_start();
        if bang {
            if !arg.trim().is_empty() {
                return Err("E474: Invalid argument".into());
            }
            self.clear_all_marks();
            return Ok(());
        }
        if arg.trim().is_empty() {
            return Err("E471: Argument required".into());
        }
        let chars: Vec<char> = arg.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == ' ' || c == '\t' {
                i += 1;
                continue;
            }
            if chars.get(i + 1) == Some(&'-') {
                // A range: both ends must be lowercase, uppercase or digits.
                let end = chars.get(i + 2).copied();
                let same_kind = match end {
                    Some(e) => {
                        (c.is_ascii_lowercase() && e.is_ascii_lowercase())
                            || (c.is_ascii_uppercase() && e.is_ascii_uppercase())
                            || (c.is_ascii_digit() && e.is_ascii_digit())
                    }
                    None => false,
                };
                let Some(e) = end.filter(|&e| same_kind && c <= e) else {
                    return Err(format!("E475: Invalid argument: {arg}"));
                };
                for x in c..=e {
                    self.delete_mark(x);
                }
                i += 3;
            } else {
                let valid = c.is_ascii_alphabetic() || is_other_mark(c);
                if !valid {
                    let rest: String = chars[i..].iter().collect();
                    return Err(format!("E475: Invalid argument: {rest}"));
                }
                self.delete_mark(c);
                i += 1;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::buffer::Buffer;
    use crate::document::DocId;
    use crate::editor::Editor;
    use crate::marks::MAXCOL;
    use crate::motion::Pos;
    use crate::undo::UndoSource;

    #[test]
    fn special_marks_follow_an_external_reload() {
        let mut ed = Editor::from_text("one\ntwo\nthree\nfour");
        // `yy` sets '[ and '] on line 1; a Visual area on lines 3-4 sets '< and '>.
        ed.feed("yyjjVj<Esc>");
        assert_eq!(ed.special_mark('['), Some(Pos::new(0, 0)));
        assert_eq!(ed.special_mark('<'), Some(Pos::new(2, 0)));
        // Linewise area: '> is at the end of its line (MAXCOL).
        assert_eq!(ed.special_mark('>'), Some(Pos::new(3, MAXCOL)));
        // A line is inserted above: every mark moves down one line.
        let new = Buffer::from_text("zero\none\ntwo\nthree\nfour");
        ed.apply_external_change(DocId::default(), new, UndoSource::External);
        assert_eq!(ed.special_mark('['), Some(Pos::new(1, 0)));
        // ']' was the end of the yanked line (past its last character),
        // clamped to the last character of "one".
        assert_eq!(ed.special_mark(']'), Some(Pos::new(1, 2)));
        assert_eq!(ed.special_mark('<'), Some(Pos::new(3, 0)));
        assert_eq!(ed.special_mark('>'), Some(Pos::new(4, MAXCOL)));
        // Unset marks stay unset.
        let mut fresh = Editor::from_text("one\ntwo");
        let new = Buffer::from_text("zero\none\ntwo");
        fresh.apply_external_change(DocId::default(), new, UndoSource::External);
        assert_eq!(fresh.special_mark('['), None);
        assert_eq!(fresh.special_mark('<'), None);
    }

    fn msg(ed: &Editor) -> String {
        ed.message().map(|m| m.text.clone()).unwrap_or_default()
    }

    #[test]
    fn marks_listing_rows_and_order() {
        let mut ed = Editor::from_text("abc def\nghi jkl\nmno\n");
        ed.feed("jllmajmbgg");
        ed.feed(":marks<CR>");
        assert_eq!(
            msg(&ed),
            "mark line  col file/text | '      3    2 mno | a      2    2 ghi jkl \
             | b      3    2 mno | \"      1    0 abc def"
        );
    }

    #[test]
    fn marks_listing_error_when_nothing_matches() {
        let mut ed = Editor::from_text("abc\n");
        ed.feed(":marks x<CR>");
        assert_eq!(msg(&ed), "E283: No marks matching \"x\"");
    }

    #[test]
    fn delmarks_errors() {
        let mut ed = Editor::from_text("abc\n");
        ed.feed(":delm<CR>");
        assert_eq!(msg(&ed), "E471: Argument required");
        ed.feed(":delm a-<CR>");
        assert_eq!(msg(&ed), "E475: Invalid argument: a-");
        ed.feed(":delm *<CR>");
        assert_eq!(msg(&ed), "E475: Invalid argument: *");
        ed.feed(":delm! a<CR>");
        assert_eq!(msg(&ed), "E474: Invalid argument");
    }

    #[test]
    fn file_marks_listing_and_errors() {
        let mut ed = Editor::from_text("abc def\nghi jkl\nmno\n");
        ed.feed("jmAgg:marks A<CR>");
        assert_eq!(msg(&ed), "mark line  col file/text | A      2    0 ghi jkl");
        // Unset file marks: E20; an unknown mark name: E78.
        ed.feed("gg'Q");
        assert_eq!(msg(&ed), "E20: Mark not set");
        ed.feed("gg`*");
        assert_eq!(msg(&ed), "E78: Unknown mark");
        // Deleting the marked last line leaves the mark past the end: E19.
        ed.feed("GmAdd");
        ed.feed("gg'A");
        assert_eq!(msg(&ed), "E19: Mark has invalid line number");
        // `:delmarks A` unsets it, and `:delmarks!` leaves file marks alone.
        let mut ed = Editor::from_text("abc\ndef\n");
        ed.feed("jmA:delmarks!<CR>");
        assert_eq!(ed.buf().mark('A'), Some(Pos::new(1, 0)));
        ed.feed(":delmarks A<CR>");
        assert_eq!(ed.buf().mark('A'), None);
    }
}

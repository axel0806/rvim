//! Turns editor state into screen rows: scrolling, line wrapping ('wrap' is
//! on), `~` filler rows, `@` rows for a partly visible last line, and the
//! command/ruler line. Pure functions so they can be tested without a
//! terminal.

use crate::editor::{Editor, Mode};
use crate::text::{char_display, char_width};
use ropey::RopeSlice;
use unicode_width::UnicodeWidthChar;

#[derive(Default)]
pub struct View {
    /// First buffer line shown.
    pub top: usize,
}

pub struct Frame {
    /// `height - 1` text rows, each exactly `width` cells wide.
    pub rows: Vec<String>,
    /// Bottom row (command line, messages, ruler), `width` cells wide.
    pub status: String,
    pub status_is_error: bool,
    /// Rows from this index on are `~` filler past the end of the buffer.
    pub filler_from: usize,
    /// Screen (column, row) of the cursor.
    pub cursor: (usize, usize),
}

/// One buffer line laid out across screen rows.
struct Layout {
    rows: Vec<String>,
    /// Linear cell index where each char starts, plus one entry for the
    /// end-of-line position.
    starts: Vec<usize>,
    /// Width (cells) of each char.
    widths: Vec<usize>,
}

fn layout(line: RopeSlice, width: usize) -> Layout {
    let mut rows = vec![String::new()];
    let mut cell = 0usize;
    // A char that does not fit at the end of a row (a wide char) moves to the
    // next row; Vim fills the gap with '>'.
    let align = |rows: &mut Vec<String>, cell: &mut usize, w: usize| {
        if *cell % width + w > width {
            while !(*cell).is_multiple_of(width) {
                rows.last_mut().unwrap().push('>');
                *cell += 1;
            }
        }
    };
    let mut starts = Vec::new();
    let mut widths = Vec::new();
    let mut vcol = 0;
    for c in line.chars() {
        let w = char_width(c, vcol);
        let shown = char_display(c, vcol);
        let first_w = shown.chars().next().map_or(1, cell_width);
        align(&mut rows, &mut cell, first_w);
        starts.push(cell);
        widths.push(w);
        for ch in shown.chars() {
            let cw = cell_width(ch);
            align(&mut rows, &mut cell, cw);
            if cell / width >= rows.len() {
                rows.push(String::new());
            }
            rows.last_mut().unwrap().push(ch);
            cell += cw;
        }
        vcol += w;
    }
    starts.push(cell);
    widths.push(1);
    Layout {
        rows,
        starts,
        widths,
    }
}

fn cell_width(c: char) -> usize {
    c.width().unwrap_or(1).max(1)
}

fn pad(mut s: String, width: usize) -> String {
    let used: usize = s.chars().map(cell_width).sum();
    for _ in used..width {
        s.push(' ');
    }
    s
}

/// Rows the line needs, including a row for an Insert-mode cursor sitting
/// just past a line that exactly fills its last row.
fn rows_needed(lay: &Layout, width: usize) -> usize {
    let end = *lay.starts.last().unwrap();
    lay.rows.len().max(end / width + 1)
}

impl View {
    /// Scrolls so the whole cursor line is visible.
    pub fn scroll(&mut self, ed: &Editor, width: usize, text_height: usize) {
        let cur = ed.cursor().line;
        if cur < self.top {
            self.top = cur;
            return;
        }
        let rows: Vec<usize> = (self.top..=cur)
            .map(|l| rows_needed(&layout(ed.buf.line(l), width), width))
            .collect();
        let mut total: usize = rows.iter().sum();
        let mut i = 0;
        while total > text_height && self.top < cur {
            total -= rows[i];
            i += 1;
            self.top += 1;
        }
    }

    pub fn frame(&mut self, ed: &Editor, width: usize, height: usize) -> Frame {
        let width = width.max(1);
        let text_height = height.saturating_sub(1).max(1);
        self.scroll(ed, width, text_height);

        let mut rows = Vec::with_capacity(text_height);
        let mut cursor = (0, 0);
        let mut line = self.top;
        while rows.len() < text_height && line < ed.buf.line_count() {
            let lay = layout(ed.buf.line(line), width);
            let need = rows_needed(&lay, width);
            if line != self.top && rows.len() + need > text_height {
                while rows.len() < text_height {
                    rows.push(pad("@".into(), width));
                }
                break;
            }
            if line == ed.cursor().line {
                let col = ed.cursor().col.min(lay.starts.len() - 1);
                let mut cell = lay.starts[col];
                let on_tab = ed.buf.char_at(line, col) == Some('\t');
                if on_tab && ed.mode() != Mode::Insert {
                    cell += lay.widths[col] - 1;
                }
                cursor = (cell % width, rows.len() + cell / width);
            }
            for r in 0..need {
                if rows.len() == text_height {
                    break;
                }
                let text = lay.rows.get(r).cloned().unwrap_or_default();
                rows.push(pad(text, width));
            }
            line += 1;
        }
        let filler_from = rows.len();
        while rows.len() < text_height {
            rows.push(pad("~".into(), width));
        }

        // `line` is now the first buffer line not fully shown.
        let below = ed.buf.line_count() - line;
        let (status, status_is_error) = self.status_line(ed, width, &mut cursor, height, below);
        Frame {
            rows,
            status,
            status_is_error,
            filler_from,
            cursor,
        }
    }

    fn status_line(
        &self,
        ed: &Editor,
        width: usize,
        cursor: &mut (usize, usize),
        height: usize,
        below: usize,
    ) -> (String, bool) {
        let bottom = height.saturating_sub(1);
        if ed.mode() == Mode::CmdLine {
            let text = format!(":{}", ed.cmdline());
            let used: usize = text.chars().map(|c| char_width(c, 0)).sum();
            *cursor = (used.min(width - 1), bottom);
            return (pad(truncate(&text, width), width), false);
        }
        let (left, error) = match (ed.mode(), ed.message()) {
            (Mode::Insert, _) => ("-- INSERT --".to_string(), false),
            (_, Some(m)) => (m.text.clone(), m.error),
            _ => (String::new(), false),
        };
        let ruler = self.ruler(ed, below);
        let ruler_col = width.saturating_sub(18);
        let mut s = truncate(&left, ruler_col.saturating_sub(1).max(1));
        s = pad(s, ruler_col);
        s.push_str(&ruler);
        (pad(truncate(&s, width), width), error)
    }

    /// Vim's ruler: `line,col[-vcol]` and the position in the file.
    fn ruler(&self, ed: &Editor, below: usize) -> String {
        let cur = ed.cursor();
        let line = ed.buf.line(cur.line);
        let col = if line.len_chars() == 0 {
            "0-1".to_string()
        } else {
            let byte = line.slice(..cur.col.min(line.len_chars())).len_bytes() + 1;
            let vcol = ed.cursor_vcol() + 1;
            if byte == vcol {
                format!("{byte}")
            } else {
                format!("{byte}-{vcol}")
            }
        };
        let above = self.top;
        let pos = match (above, below) {
            (0, 0) => "All".to_string(),
            (0, _) => "Top".to_string(),
            (_, 0) => "Bot".to_string(),
            _ => format!("{}%", above * 100 / (above + below)),
        };
        format!("{:<14}{pos}", format!("{},{col}", cur.line + 1))
    }
}

fn truncate(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = cell_width(c);
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_scrolls() {
        let mut ed = Editor::from_text("abcdefghij\nx\ny\nz");
        let mut v = View::default();
        let f = v.frame(&ed, 4, 4);
        assert_eq!(f.rows, ["abcd", "efgh", "ij  "]);
        ed.feed("G");
        let f = v.frame(&ed, 4, 4);
        assert_eq!(v.top, 1);
        assert_eq!(f.rows, ["x   ", "y   ", "z   "]);
        assert_eq!(f.cursor, (0, 2));
    }

    #[test]
    fn tab_cursor_and_partial_lines() {
        let mut ed = Editor::from_text("\tx\nabcdefghijklmnop");
        let mut v = View::default();
        let f = v.frame(&ed, 10, 3);
        assert_eq!(f.rows, ["        x ", "@         "]);
        assert_eq!(f.cursor, (7, 0));
        ed.feed("i");
        assert_eq!(v.frame(&ed, 10, 3).cursor, (0, 0));
    }
}

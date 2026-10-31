//! Display widths and virtual columns ('tabstop' 8, Vim's control-character
//! notation). Shared by cursor movement and rendering so they always agree.

use ropey::RopeSlice;
use unicode_width::UnicodeWidthChar;

pub const TABSTOP: usize = 8;

/// Cells `c` occupies when it starts at virtual column `vcol`.
pub fn char_width(c: char, vcol: usize) -> usize {
    match c {
        '\t' => TABSTOP - vcol % TABSTOP,
        c if (c as u32) < 0x20 || c == '\x7f' => 2,
        c if ('\u{80}'..'\u{a0}').contains(&c) => 4,
        c => c.width().unwrap_or(1).max(1),
    }
}

/// What to draw for `c` at `vcol`. The result is exactly `char_width` cells.
pub fn char_display(c: char, vcol: usize) -> String {
    match c {
        '\t' => " ".repeat(char_width(c, vcol)),
        c if (c as u32) < 0x20 || c == '\x7f' => {
            format!("^{}", char::from_u32(c as u32 ^ 0x40).unwrap_or('?'))
        }
        c if ('\u{80}'..'\u{a0}').contains(&c) => format!("<{:02x}>", c as u32),
        c if c.width() == Some(0) => " ".to_string(),
        c => c.to_string(),
    }
}

/// First and last virtual column of char `col` (a position past the end
/// occupies one cell).
pub fn vcol_range(line: RopeSlice, col: usize) -> (usize, usize) {
    let mut vcol = 0;
    for (i, c) in line.chars().enumerate() {
        let w = char_width(c, vcol);
        if i == col {
            return (vcol, vcol + w - 1);
        }
        vcol += w;
    }
    (vcol, vcol)
}

/// The virtual column the cursor is shown at. In Normal mode it sits on the
/// last cell of a tab, otherwise on the first cell.
pub fn cursor_vcol(line: RopeSlice, col: usize, insert: bool) -> usize {
    let (start, end) = vcol_range(line, col);
    let on_tab = col < line.len_chars() && line.char(col) == '\t';
    if on_tab && !insert { end } else { start }
}

/// The char covering virtual column `want`. Past the end this is the last
/// char, or the end-of-line position when `allow_end` (Insert mode).
pub fn col_for_vcol(line: RopeSlice, want: usize, allow_end: bool) -> usize {
    let mut vcol = 0;
    for (i, c) in line.chars().enumerate() {
        vcol += char_width(c, vcol);
        if vcol > want {
            return i;
        }
    }
    let len = line.len_chars();
    if allow_end {
        len
    } else {
        len.saturating_sub(1)
    }
}

/// Total display width of a line.
pub fn line_width(line: RopeSlice) -> usize {
    line.chars().fold(0, |vcol, c| vcol + char_width(c, vcol))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    #[test]
    fn tabs_and_wide_chars() {
        let r = Rope::from_str("a\tb漢c");
        let l = r.slice(..);
        assert_eq!(vcol_range(l, 1), (1, 7));
        assert_eq!(cursor_vcol(l, 1, false), 7);
        assert_eq!(cursor_vcol(l, 1, true), 1);
        assert_eq!(vcol_range(l, 3), (9, 10));
        assert_eq!(col_for_vcol(l, 4, false), 1);
        assert_eq!(col_for_vcol(l, 10, false), 3);
        assert_eq!(col_for_vcol(l, 99, false), 4);
        assert_eq!(col_for_vcol(l, 99, true), 5);
        assert_eq!(line_width(l), 12);
        assert_eq!(char_display('\x01', 0), "^A");
    }
}

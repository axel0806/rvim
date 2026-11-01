//! Display widths and virtual columns (Vim's control-character notation and
//! tabs of width 'tabstop'). Every function takes the tabstop `ts` as an
//! argument, so cursor movement and rendering always agree.

use ropey::RopeSlice;
use unicode_width::UnicodeWidthChar;

/// Cells `c` occupies when it starts at virtual column `vcol`.
pub fn char_width(c: char, vcol: usize, ts: usize) -> usize {
    match c {
        '\t' => ts - vcol % ts,
        c if (c as u32) < 0x20 || c == '\x7f' => 2,
        c if ('\u{80}'..'\u{a0}').contains(&c) => 4,
        c => c.width().unwrap_or(1).max(1),
    }
}

/// What to draw for `c` at `vcol`. The result is exactly `char_width` cells.
pub fn char_display(c: char, vcol: usize, ts: usize) -> String {
    match c {
        '\t' => " ".repeat(char_width(c, vcol, ts)),
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
pub fn vcol_range(line: RopeSlice, col: usize, ts: usize) -> (usize, usize) {
    let mut vcol = 0;
    for (i, c) in line.chars().enumerate() {
        let w = char_width(c, vcol, ts);
        if i == col {
            return (vcol, vcol + w - 1);
        }
        vcol += w;
    }
    (vcol, vcol)
}

/// The virtual column the cursor is shown at. In Normal mode it sits on the
/// last cell of a tab, otherwise on the first cell.
pub fn cursor_vcol(line: RopeSlice, col: usize, insert: bool, ts: usize) -> usize {
    let (start, end) = vcol_range(line, col, ts);
    let on_tab = col < line.len_chars() && line.char(col) == '\t';
    if on_tab && !insert { end } else { start }
}

/// The char covering virtual column `want`. Past the end this is the last
/// char, or the end-of-line position when `allow_end` (Insert mode).
pub fn col_for_vcol(line: RopeSlice, want: usize, allow_end: bool, ts: usize) -> usize {
    let mut vcol = 0;
    for (i, c) in line.chars().enumerate() {
        vcol += char_width(c, vcol, ts);
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
pub fn line_width(line: RopeSlice, ts: usize) -> usize {
    line.chars()
        .fold(0, |vcol, c| vcol + char_width(c, vcol, ts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    #[test]
    fn tabs_and_wide_chars() {
        let r = Rope::from_str("a\tb漢c");
        let l = r.slice(..);
        assert_eq!(vcol_range(l, 1, 8), (1, 7));
        assert_eq!(cursor_vcol(l, 1, false, 8), 7);
        assert_eq!(cursor_vcol(l, 1, true, 8), 1);
        assert_eq!(vcol_range(l, 3, 8), (9, 10));
        assert_eq!(col_for_vcol(l, 4, false, 8), 1);
        assert_eq!(col_for_vcol(l, 10, false, 8), 3);
        assert_eq!(col_for_vcol(l, 99, false, 8), 4);
        assert_eq!(col_for_vcol(l, 99, true, 8), 5);
        assert_eq!(line_width(l, 8), 12);
        assert_eq!(char_display('\x01', 0, 8), "^A");
    }

    #[test]
    fn tabstop_is_a_parameter() {
        let r = Rope::from_str("a\tb");
        let l = r.slice(..);
        assert_eq!(vcol_range(l, 1, 4), (1, 3));
        assert_eq!(vcol_range(l, 2, 4), (4, 4));
        assert_eq!(line_width(l, 2), 3);
        assert_eq!(col_for_vcol(l, 1, false, 2), 1);
        assert_eq!(col_for_vcol(l, 2, false, 2), 2);
    }
}

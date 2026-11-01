// SPDX-License-Identifier: Apache-2.0
//! Line numbers: 'number', 'relativenumber' and 'numberwidth' (window-local).
//! Checked on the Frame, because `vim -es` has no screen to compare with: the
//! rules are Vim's `number_width()` and `win_col_off()` (the column is the
//! digits plus one space; with only 'relativenumber' it is sized by the window
//! height; the cursor line of 'number' with 'relativenumber' is left-aligned).
//! Wrapped rows get blanks in the number column, and the text area that wraps
//! and scrolls is narrower by the column's width.

use rvim::Editor;
use rvim::view::{Hl, frame};

/// An editor with `text` on a `cols` by `rows` screen (the last row is the
/// command line, so `rows - 1` text rows).
fn ed(text: &str, cols: usize, rows: usize) -> Editor {
    let mut ed = Editor::from_text(text);
    ed.set_screen_size(cols, rows);
    ed
}

/// `s` padded with spaces to `width` cells, as the Frame rows are.
fn row(s: &str, width: usize) -> String {
    format!("{s:<width$}")
}

#[test]
fn default_output_is_unchanged() {
    // Both options are off: no number column, the text starts at column 0.
    let e = ed("abc\ndef", 10, 4);
    let f = frame(&e);
    assert_eq!(f.rows, [row("abc", 10), row("def", 10), row("~", 10)]);
    assert_eq!(f.cursor, (0, 0));
    assert_eq!(f.filler_from, 2);
}

#[test]
fn number_shows_absolute_numbers() {
    let mut e = ed("a\nb\nc", 10, 4);
    e.feed(":set nu<CR>");
    let f = frame(&e);
    // numberwidth 4: three digits right-aligned, then a space.
    assert_eq!(
        f.rows,
        [row("  1 a", 10), row("  2 b", 10), row("  3 c", 10)]
    );
    // The cursor is on line 1, after the number column.
    assert_eq!(f.cursor, (4, 0));
}

#[test]
fn relativenumber_shows_distances_and_zero_on_the_cursor_line() {
    let mut e = ed("a\nb\nc\nd", 10, 5);
    e.feed(":set rnu<CR>j");
    let f = frame(&e);
    assert_eq!(
        f.rows,
        [
            row("  1 a", 10),
            row("  0 b", 10),
            row("  1 c", 10),
            row("  2 d", 10)
        ]
    );
    assert_eq!(f.cursor, (4, 1));
}

#[test]
fn number_and_relativenumber_show_the_absolute_number_left_aligned_on_the_cursor_line() {
    let mut e = ed("a\nb\nc\nd", 10, 5);
    e.feed(":set nu rnu<CR>j");
    let f = frame(&e);
    assert_eq!(
        f.rows,
        [
            row("  1 a", 10),
            row("2   b", 10),
            row("  1 c", 10),
            row("  2 d", 10)
        ]
    );
}

#[test]
fn nonu_restores_the_default_output() {
    let mut e = ed("abc\ndef", 10, 4);
    let before = frame(&e).rows;
    e.feed(":set nu<CR>:set nonu<CR>");
    assert_eq!(frame(&e).rows, before);
    assert_eq!(frame(&e).cursor, (0, 0));
}

#[test]
fn wrapped_lines_are_narrower_and_their_later_rows_are_blank_in_the_column() {
    // Width 10, column 4: the text wraps at 6 cells.
    let mut e = ed("abcdefghijklmn", 10, 4);
    e.feed(":set nu<CR>$");
    let f = frame(&e);
    assert_eq!(
        f.rows,
        [
            row("  1 abcdef", 10),
            row("    ghijkl", 10),
            row("    mn", 10)
        ]
    );
    // Column 13 is the 2nd cell of the third row, after the column.
    assert_eq!(f.cursor, (5, 2));
    // The number column of a continuation row is not highlighted.
    assert_eq!(f.hl[1][..4], [Hl::Normal; 4]);
}

#[test]
fn a_wrapped_window_scrolls_with_the_narrower_text_area() {
    // Five lines of 14 cells: two rows each at 10 columns, three at 6 (with
    // the number column). The window has five text rows.
    let text = ["aaaaaaaaaaaaaa"; 5].join("\n");
    let mut plain = ed(&text, 10, 6);
    plain.feed("G");
    assert_eq!(plain.top_line(), 3, "two lines of two rows fit");
    let mut numbered = ed(&text, 10, 6);
    numbered.feed(":set nu<CR>G");
    assert_eq!(numbered.top_line(), 4, "one line of three rows fits");
    // The cursor line is the window's first row, after the column.
    assert_eq!(frame(&numbered).cursor, (4, 0));
}

#[test]
fn the_number_column_widens_past_999_lines() {
    let text = vec!["x"; 1000].join("\n");
    let mut e = ed(&text, 12, 4);
    e.feed(":set nu<CR>");
    assert_eq!(frame(&e).rows[0], row("   1 x", 12), "four digits, width 5");
    let short = vec!["x"; 999].join("\n");
    let mut e = ed(&short, 12, 4);
    e.feed(":set nu<CR>");
    assert_eq!(frame(&e).rows[0], row("  1 x", 12), "three digits, width 4");
}

#[test]
fn numberwidth_sets_the_least_width_and_is_clamped_to_one_to_twenty() {
    let mut e = ed("a\nb", 10, 4);
    e.feed(":set nu nuw=6<CR>");
    assert_eq!(
        frame(&e).rows[..2],
        [row("    1 a", 10), row("    2 b", 10)]
    );
    assert_eq!(frame(&e).cursor, (6, 0));
    e.feed(":set nuw=21<CR>:set nuw?<CR>");
    assert_eq!(e.message().unwrap().text.trim(), "numberwidth=20");
}

#[test]
fn the_filler_rows_have_a_blank_number_column() {
    let mut e = ed("a", 10, 4);
    e.feed(":set nu<CR>");
    let f = frame(&e);
    assert_eq!(f.rows[1], row("    ~", 10));
    assert_eq!(f.hl[1][..4], [Hl::Normal; 4]);
    assert_eq!(f.hl[1][4], Hl::NonText);
}

#[test]
fn number_cells_carry_the_line_number_groups() {
    let mut e = ed("a\nb", 10, 4);
    e.feed(":set nu<CR>j");
    let f = frame(&e);
    assert_eq!(f.hl[0][..4], [Hl::LineNr; 4]);
    assert_eq!(f.hl[1][..4], [Hl::CursorLineNr; 4]);
    assert_eq!(f.hl[1][4], Hl::Normal, "the text is not highlighted");
}

#[test]
fn visual_selection_is_drawn_in_the_text_area() {
    let mut e = ed("abc", 10, 4);
    e.feed(":set nu<CR>vl");
    let f = frame(&e);
    assert_eq!(f.hl[0][3], Hl::CursorLineNr);
    assert_eq!(f.hl[0][4], Hl::Visual);
    assert_eq!(f.hl[0][5], Hl::Visual);
    assert_eq!(f.hl[0][6], Hl::Normal);
}

#[test]
fn options_show_set_and_toggle_as_vim_does() {
    let mut e = ed("a", 10, 4);
    e.feed(":set nu?<CR>");
    assert_eq!(e.message().unwrap().text.trim(), "nonumber");
    e.feed(":set nu<CR>:set nu?<CR>");
    assert_eq!(e.message().unwrap().text.trim(), "number");
    e.feed(":set invnu<CR>:set nu?<CR>");
    assert_eq!(e.message().unwrap().text.trim(), "nonumber");
    e.feed(":setlocal nu<CR>:setlocal nu?<CR>");
    assert_eq!(e.message().unwrap().text.trim(), "number");
    e.feed(":set rnu<CR>:set rnu!<CR>:set rnu?<CR>");
    assert_eq!(e.message().unwrap().text.trim(), "norelativenumber");
}

//! Scrolling commands checked against Vim (CTRL-E/Y/F/B/D/U, zt/zz/zb,
//! z<CR>/z./z-, H/M/L with counts and as operator motions, 'scrolloff').
//!
//! The harness only compares lines and the cursor, and only the cursor is
//! visible here. Vim run without a terminal (`-es`) does not update the window
//! after a cursor move, so the top line it reports is not the screen's; a
//! case is only kept when the cursor it gives is the cursor a real screen
//! gives. Top lines, wrapped lines and the scrolloff margin are checked in
//! tests/scroll_window.rs. See PLAN.md, "Scrolling and the harness".

mod common;
use common::{check, cross};

#[test]
fn desired_column_survives_line_scrolling_only() {
    // CTRL-E/Y and the z commands keep the column (curswant); CTRL-D/U and
    // CTRL-F/B go to the first non-blank. The lines are all the same width.
    let texts = [text(100, 0)];
    check(&cross(
        &texts,
        &[
            "4l<C-e>", "4l<C-y>", "4l<C-e>j", "$<C-e>", "4l<C-d>", "4l<C-f>", "4lzt", "4lzz",
            "4lzb", "4lz<CR>", "V<C-d>",
        ],
    ));
}

/// `n` lines of the form `  line1xxx` (leading blanks, so first non-blank
/// matters), each `len` characters long.
fn text(n: usize, len: usize) -> &'static str {
    let filler = "x".repeat(len);
    let s: String = (1..=n)
        .map(|i| format!("  line{i}{filler}"))
        .collect::<Vec<_>>()
        .join("\n");
    Box::leak(s.into_boxed_str())
}

#[test]
fn page_and_half_page_scrolling() {
    let texts = [text(100, 0)];
    check(&cross(
        &texts,
        &[
            "<C-f>",
            "<C-f><C-f>",
            "2<C-f>",
            "<C-b>",
            "<C-d>",
            "<C-d><C-d>",
            "3<C-d><C-d>",
            "<C-d><C-u>",
            "<C-u>",
            "<C-d><C-y>",
            "<C-e>",
            "3<C-e>",
            "<C-y>",
            "5<C-e>",
        ],
    ));
}

#[test]
fn screen_lines_with_counts() {
    let texts = [text(100, 0)];
    check(&cross(
        &texts,
        &[
            "H", "5H", "30H", "M", "L", "3L", "30L", "dH", "dM", "dL", "yL", "Vjd", "d5H", "5dL",
        ],
    ));
}

#[test]
fn cursor_scroll_commands() {
    let texts = [text(100, 0)];
    check(&cross(
        &texts,
        &[
            "zt", "zz", "zb", "z<CR>", "z.", "z-", "5zt", "5z<CR>", "5z.", "5z-", "5zz", "50zt",
            "10jzt",
        ],
    ));
}

#[test]
fn scrolloff_moves_the_cursor_inside_the_margin() {
    let texts = [text(100, 0)];
    check(&cross(
        &texts,
        &[
            ":set so=5<CR>L",
            ":set so=5<CR>H",
            ":set so=5<CR>3L",
            ":set so=5<CR>5H",
            ":set so=5<CR>M",
            ":set so=5<CR>10zt",
            ":set so=5<CR><C-d>",
        ],
    ));
}

#[test]
fn wrapped_lines_in_the_harness_window() {
    // 200 characters wrap to three rows at 80 columns; the cursor effects
    // of the first screen are compared, the scroll offset is in unit tests.
    let texts = [text(30, 190)];
    check(&cross(&texts, &["L", "M", "H", "3H", "<C-d>", "<C-e>"]));
}

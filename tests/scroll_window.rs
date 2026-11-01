// SPDX-License-Identifier: Apache-2.0
//! Window top line, scrolling with wrapped lines, 'scrolloff' and resizing.
//! These are what the differential harness cannot see (Vim run without a
//! terminal does not keep its window up to date; see PLAN.md, "Scrolling and
//! the harness"). The window is 23 text rows by 80 columns, Vim's default
//! without a terminal. Lines are 0-based here; Vim's are 1-based.

use rvim::Editor;
use rvim::view::frame;

fn lines(n: usize) -> String {
    (1..=n)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// An editor with `n` short lines and the default 80x24 screen.
fn editor(n: usize) -> Editor {
    Editor::from_text(&lines(n))
}

/// Lines `long` characters wide: three screen rows each at 80 columns.
fn wrapped(n: usize, long: usize) -> Editor {
    let text: Vec<String> = (1..=n)
        .map(|i| format!("{i:03}{}", "x".repeat(long)))
        .collect();
    Editor::from_text(&text.join("\n"))
}

#[test]
fn page_down_and_up_move_the_window_by_two_lines_less_than_a_page() {
    let mut ed = editor(100);
    ed.feed("<C-f>");
    assert_eq!(ed.top_line(), 21);
    assert_eq!(ed.cursor().line, 21);
    ed.feed("<C-f>");
    assert_eq!(ed.top_line(), 42);
    ed.feed("<C-b>");
    assert_eq!(ed.top_line(), 21);
    assert_eq!(frame(&ed).rows[0].trim_end(), "line22");
}

#[test]
fn half_page_scrolls_the_window_and_the_cursor_together() {
    let mut ed = editor(100);
    ed.feed("<C-d>");
    assert_eq!((ed.top_line(), ed.cursor().line), (11, 11));
    ed.feed("<C-u>");
    assert_eq!((ed.top_line(), ed.cursor().line), (0, 0));
    // A count sets 'scroll' and stays for the next CTRL-D.
    ed.feed("3<C-d>");
    assert_eq!((ed.top_line(), ed.cursor().line), (3, 3));
    ed.feed("<C-d>");
    assert_eq!((ed.top_line(), ed.cursor().line), (6, 6));
}

#[test]
fn line_scrolling_keeps_the_cursor_in_the_window() {
    let mut ed = editor(100);
    ed.feed("5<C-e>");
    assert_eq!((ed.top_line(), ed.cursor().line), (5, 5));
    // The cursor stays where it is while it is still shown.
    ed.feed("5<C-y>");
    assert_eq!((ed.top_line(), ed.cursor().line), (0, 5));
    ed.feed("<C-y>");
    assert_eq!(ed.top_line(), 0);
}

#[test]
fn cursor_scroll_commands_place_the_line() {
    let mut ed = editor(100);
    ed.feed("50zt");
    assert_eq!((ed.top_line(), ed.cursor().line), (49, 49));
    ed.feed("50zz");
    assert_eq!((ed.top_line(), ed.cursor().line), (38, 49));
    ed.feed("50zb");
    assert_eq!((ed.top_line(), ed.cursor().line), (27, 49));
}

#[test]
fn z_enter_moves_to_the_first_non_blank() {
    let mut ed = Editor::from_text("a\n  b\nc");
    ed.feed("jz<CR>");
    assert_eq!((ed.cursor().line, ed.cursor().col), (1, 2));
    let mut ed = Editor::from_text("a\n  b\nc");
    ed.feed("jzt");
    assert_eq!((ed.cursor().line, ed.cursor().col), (1, 0));
}

#[test]
fn far_jumps_centre_the_cursor_and_near_ones_scroll_the_minimum() {
    let mut ed = editor(100);
    ed.feed("30j");
    assert_eq!(ed.top_line(), 8);
    let mut ed = editor(100);
    ed.feed("50j");
    assert_eq!((ed.top_line(), ed.cursor().line), (39, 50));
    let mut ed = editor(100);
    ed.feed("G");
    assert_eq!((ed.top_line(), ed.cursor().line), (77, 99));
}

#[test]
fn screen_lines_are_inside_the_window() {
    let mut ed = editor(100);
    ed.feed("M");
    assert_eq!(ed.cursor().line, 11);
    ed.feed("L");
    assert_eq!(ed.cursor().line, 22);
    ed.feed("H");
    assert_eq!(ed.cursor().line, 0);
    ed.feed("3L");
    assert_eq!(ed.cursor().line, 20);
    ed.feed("5H");
    assert_eq!(ed.cursor().line, 4);
    // A count past the window is clamped to it.
    ed.feed("30H");
    assert_eq!(ed.cursor().line, 22);
}

#[test]
fn screen_lines_follow_the_window_after_scrolling() {
    let mut ed = editor(100);
    ed.feed("<C-f>");
    ed.feed("H");
    assert_eq!(ed.cursor().line, 21);
    ed.feed("L");
    assert_eq!(ed.cursor().line, 43);
}

#[test]
fn scrolloff_keeps_context_lines_around_the_cursor() {
    let mut ed = editor(100);
    ed.feed(":set so=5<CR>");
    ed.feed("22j");
    assert_eq!((ed.top_line(), ed.cursor().line), (5, 22));
    // L stops above the bottom margin. H stops below the top margin, but
    // only when the window does not start at the top of the file.
    let mut ed = editor(100);
    ed.feed(":set so=5<CR>L");
    assert_eq!(ed.cursor().line, 17);
    let mut ed = editor(100);
    ed.feed(":set so=5<CR>22jH");
    assert_eq!(ed.cursor().line, 10);
    let mut ed = editor(100);
    ed.feed(":set so=5<CR>2H");
    assert_eq!(ed.cursor().line, 1);
    let mut ed = editor(100);
    ed.feed(":set so=5<CR>G");
    assert_eq!((ed.top_line(), ed.cursor().line), (77, 99));
}

#[test]
fn scrolloff_is_capped_at_half_the_window() {
    let mut ed = editor(100);
    ed.feed(":set so=99<CR>");
    // (23 - 1) / 2 = 11 rows of margin: the cursor is always the middle line.
    ed.feed("30j");
    assert_eq!(ed.cursor().line, 30);
    assert_eq!(ed.top_line(), 19);
}

#[test]
fn wrapped_lines_count_their_screen_rows() {
    // 30 lines of three rows each: 23 rows show seven whole lines.
    let mut ed = wrapped(30, 190);
    assert_eq!(rvim::view::line_height(ed.buf().line(0), 80, 8), 3);
    ed.feed("M");
    assert_eq!(ed.cursor().line, 3);
    ed.feed("L");
    assert_eq!(ed.cursor().line, 6);
    ed.feed("G");
    // Seven lines of three rows (lines 23 to 29) fill 21 of the 23 rows, and
    // an eighth line would need 24, so the window starts at line 23.
    assert_eq!((ed.top_line(), ed.cursor().line), (23, 29));
    assert_eq!(frame(&ed).rows.len(), 23);
}

#[test]
fn half_page_on_wrapped_lines_scrolls_whole_lines_within_its_rows() {
    // CTRL-D scrolls 11 screen rows: three whole lines of 3 rows, and the
    // cursor moves the same three lines (Vim's halfpage() rule).
    let mut ed = wrapped(30, 190);
    ed.feed("<C-d>");
    assert_eq!((ed.top_line(), ed.cursor().line), (3, 3));
    let mut ed = wrapped(30, 190);
    ed.feed("<C-e>");
    assert_eq!(ed.top_line(), 1);
}

#[test]
fn resize_keeps_the_cursor_in_view() {
    let mut ed = editor(100);
    ed.feed("50G");
    ed.set_screen_size(80, 10);
    let top = ed.top_line();
    let cur = ed.cursor().line;
    assert!(top <= cur && cur < top + 9, "top {top}, cursor {cur}");
    let f = frame(&ed);
    assert_eq!(f.rows.len(), 9);
    assert_eq!(f.rows[cur - top].trim_end(), format!("line{}", cur + 1));
}

#[test]
fn events_resize_the_window() {
    use rvim::Event;
    let mut ed = editor(100);
    ed.handle_event(Event::Resize(60, 12));
    ed.feed("G");
    assert_eq!(ed.top_line(), 89);
    assert_eq!(frame(&ed).rows.len(), 11);
}

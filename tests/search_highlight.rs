// SPDX-License-Identifier: Apache-2.0
//! Frame goldens for 'hlsearch' and 'incsearch', and the option behaviour
//! that shows on the screen. Vim's highlight groups are shown as one letter
//! per cell: `N` Normal, `V` Visual, `S` Search, `I` IncSearch.

use rvim::view::frame;
use rvim::{Editor, Mode};

/// The highlight of every cell of text row `r`, one letter per cell.
fn row_hl(ed: &Editor, r: usize) -> String {
    use rvim::view::Hl;
    frame(ed).hl[r]
        .iter()
        .map(|h| match h {
            Hl::Normal => 'N',
            Hl::Visual => 'V',
            Hl::Search => 'S',
            Hl::IncSearch => 'I',
            Hl::NonText => '~',
            _ => '?',
        })
        .collect()
}

/// Expected cells of a row: `s`, then Normal cells up to `width`.
fn exp(s: &str, width: usize) -> String {
    format!("{s:N<width$}")
}

fn text_rows(ed: &Editor) -> Vec<String> {
    frame(ed)
        .rows
        .iter()
        .map(|r| r.trim_end().to_string())
        .collect()
}

const FOO: &str = "foo bar foo\nxfoo\nbar";

#[test]
fn hlsearch_marks_every_match() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/foo<CR>");
    assert_eq!(row_hl(&ed, 0), exp("SSSNNNNNSSS", 20));
    assert_eq!(row_hl(&ed, 1), exp("NSSS", 20));
    assert_eq!(row_hl(&ed, 2), exp("", 20));
}

#[test]
fn hlsearch_highlights_the_last_pattern_not_the_cursor_match() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/bar<CR>");
    assert_eq!(row_hl(&ed, 0), exp("NNNNSSS", 20));
    assert_eq!(row_hl(&ed, 2), exp("SSS", 20));
}

#[test]
fn hlsearch_off_draws_no_search_cells() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    let before = frame(&ed);
    ed.feed("/foo<CR>");
    let after = frame(&ed);
    assert_eq!(before.rows, after.rows);
    assert_eq!(before.hl, after.hl);
    assert!(!(0..4).any(|r| row_hl(&ed, r).contains('S')));
}

#[test]
fn matches_on_a_wrapped_line_are_cut_at_the_row_edge() {
    let mut ed = Editor::from_text("abcdefgh\nzz");
    ed.set_screen_size(4, 6);
    ed.feed(":set hls<CR>/fg<CR>");
    assert_eq!(row_hl(&ed, 0), exp("NNNN", 4));
    assert_eq!(row_hl(&ed, 1), exp("NSSN", 4));
}

#[test]
fn a_match_across_a_row_break_is_drawn_on_both_rows() {
    let mut ed = Editor::from_text("abcdefgh\nzz");
    ed.set_screen_size(4, 6);
    ed.feed(":set hls<CR>/de<CR>");
    assert_eq!(row_hl(&ed, 0), exp("NNNS", 4));
    assert_eq!(row_hl(&ed, 1), exp("SNNN", 4));
}

#[test]
fn an_empty_match_is_one_cell() {
    let mut ed = Editor::from_text("ab\n\ncd");
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/^<CR>");
    assert_eq!(row_hl(&ed, 0), exp("SN", 20));
    // The empty line: its one cell is the cell after the line's end.
    assert_eq!(row_hl(&ed, 1), exp("S", 20));
    assert_eq!(row_hl(&ed, 2), exp("SN", 20));
}

#[test]
fn a_match_at_the_end_of_line_is_the_cell_after_the_text() {
    let mut ed = Editor::from_text("ab\ncd");
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/$<CR>");
    assert_eq!(row_hl(&ed, 0), exp("NNS", 20));
    assert_eq!(row_hl(&ed, 1), exp("NNS", 20));
}

#[test]
fn noh_clears_until_the_next_search() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/foo<CR>:noh<CR>");
    assert_eq!(row_hl(&ed, 0), exp("", 20));
    assert_eq!(row_hl(&ed, 1), exp("", 20));
    ed.feed("n");
    assert_eq!(row_hl(&ed, 0), exp("SSSNNNNNSSS", 20));
}

#[test]
fn noh_long_and_short_names_all_clear() {
    for cmd in [":noh<CR>", ":nohl<CR>", ":nohls<CR>", ":nohlsearch<CR>"] {
        let mut ed = Editor::from_text(FOO);
        ed.set_screen_size(20, 6);
        ed.feed(&format!(":set hls<CR>/foo<CR>{cmd}"));
        assert_eq!(row_hl(&ed, 1), exp("", 20), "{cmd}");
    }
}

#[test]
fn noh_is_not_undone_by_a_redraw() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/foo<CR>:noh<CR><C-l>");
    assert_eq!(row_hl(&ed, 0), exp("", 20));
}

#[test]
fn a_substitute_turns_hlsearch_back_on() {
    let mut ed = Editor::from_text("foo\nfoo");
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/foo<CR>:noh<CR>:%s/foo/foo/<CR>");
    assert_eq!(row_hl(&ed, 0), exp("SSS", 20));
}

#[test]
fn set_hls_turns_hlsearch_back_on_after_noh() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/foo<CR>:noh<CR>:set hls<CR>");
    assert_eq!(row_hl(&ed, 0), exp("SSSNNNNNSSS", 20));
}

#[test]
fn matches_under_visual_show_the_visual_area() {
    let mut ed = Editor::from_text("foo foo");
    ed.set_screen_size(20, 6);
    ed.feed(":set hls<CR>/foo<CR>0vl");
    assert_eq!(row_hl(&ed, 0), exp("VVSNSSS", 20));
    ed.feed("<Esc>");
    assert_eq!(row_hl(&ed, 0), exp("SSSNSSS", 20));
}

#[test]
fn set_hls_query_and_toggles() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set hls?<CR>");
    assert_eq!(
        ed.message().map(|m| m.text.trim().to_string()),
        Some("nohlsearch".into())
    );
    ed.feed(":set hls<CR>:set hls?<CR>");
    assert_eq!(
        ed.message().map(|m| m.text.trim().to_string()),
        Some("hlsearch".into())
    );

    ed.feed("/foo<CR>");
    assert_eq!(row_hl(&ed, 0), exp("SSSNNNNNSSS", 20));
    ed.feed(":set nohls<CR>");
    assert_eq!(row_hl(&ed, 0), exp("", 20));
    ed.feed(":set invhls<CR>");
    assert_eq!(row_hl(&ed, 0), exp("SSSNNNNNSSS", 20));
    ed.feed(":set hls!<CR>");
    assert_eq!(row_hl(&ed, 0), exp("", 20));
    ed.feed(":set hlsearch<CR>");
    assert_eq!(row_hl(&ed, 0), exp("SSSNNNNNSSS", 20));
}

#[test]
fn incsearch_highlights_the_match_while_typing() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set is<CR>/fo");
    assert_eq!(ed.mode(), Mode::CmdLine);
    assert_eq!(row_hl(&ed, 0), exp("NNNNNNNNII", 20));
    ed.feed("o");
    assert_eq!(row_hl(&ed, 0), exp("NNNNNNNNIII", 20));
    // Nothing changed in the buffer, and no hlsearch cells (it is off).
    assert_eq!(ed.buf().text(), FOO);
}

#[test]
fn incsearch_backward_from_the_end() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set is<CR>$?fo");
    assert_eq!(row_hl(&ed, 0), exp("NNNNNNNNII", 20));
}

#[test]
fn incsearch_with_no_match_draws_nothing() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set is<CR>/zz");
    assert_eq!(row_hl(&ed, 0), exp("", 20));
}

#[test]
fn incsearch_scrolls_to_show_the_match_and_escape_restores_the_view() {
    let text: Vec<String> = (0..30).map(|i| format!("row{i}")).collect();
    let text = text.join("\n");
    let mut ed = Editor::from_text(&text);
    ed.set_screen_size(20, 5);
    let before = text_rows(&ed);
    ed.feed(":set is<CR>/row25");
    assert!(text_rows(&ed).iter().any(|r| r == "row25"));
    ed.feed("<Esc>");
    assert_eq!(text_rows(&ed), before);
    assert_eq!(ed.cursor().line, 0);
    assert_eq!(ed.cursor().col, 0);
    assert_eq!(ed.mode(), Mode::Normal);
}

#[test]
fn incsearch_accept_matches_a_search_without_it() {
    let text: Vec<String> = (0..30).map(|i| format!("row{i}")).collect();
    let text = text.join("\n");
    let mut with = Editor::from_text(&text);
    with.set_screen_size(20, 5);
    with.feed(":set is<CR>/row25<CR>");
    let mut without = Editor::from_text(&text);
    without.set_screen_size(20, 5);
    without.feed("/row25<CR>");
    assert_eq!(with.cursor(), without.cursor());
    assert_eq!(text_rows(&with), text_rows(&without));
    assert_eq!(with.cursor().line, 25);
}

#[test]
fn incsearch_cancel_from_visual_returns_to_visual() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set is<CR>lv/bar<Esc>");
    assert_eq!(ed.mode(), Mode::Visual(rvim::VisualMode::Char));
    assert_eq!(ed.cursor().col, 1);
}

#[test]
fn incsearch_is_off_by_default() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed("/fo");
    assert_eq!(row_hl(&ed, 0), exp("", 20));
    assert_eq!(ed.cursor().col, 0);
}

/// Cancelling a search prompt must not change the desired column: the `j`
/// after it lands in column 4, as in Vim (Vim keeps curswant while a prompt
/// is open and does not move the cursor back on cancel).
#[test]
fn escape_and_ctrl_c_from_a_search_prompt_keep_the_desired_column() {
    let text = "abcdef\nab\nabcdef\nabcdef";
    for is in ["", ":set is<CR>"] {
        for keys in [
            "llllj/ab<Esc>",
            "llllj?abc<Esc>",
            "llllj/ab<C-c>",
            "llllj/zz<Esc>",
            "llllj/ab<BS><Esc>",
            "llllj/zz<Esc>",
        ] {
            let mut ed = Editor::from_text(text);
            ed.set_screen_size(20, 6);
            ed.feed(&format!("{is}{keys}"));
            assert_eq!(ed.mode(), Mode::Normal, "{keys}");
            assert_eq!((ed.cursor().line, ed.cursor().col), (1, 1), "{is}{keys}");
            ed.feed("jx");
            assert_eq!(ed.buf().text(), "abcdef\nab\nabcdf\nabcdef", "{is}{keys}");
        }
    }
}

/// The same after an answered prompt: `/pat<CR>` is a search, which resets
/// the desired column, so `j` lands where the search put the cursor's column.
#[test]
fn an_answered_search_prompt_resets_the_desired_column_as_a_search_does() {
    let text = "abcdef\nab\nabcdef\nabcdef";
    for is in ["", ":set is<CR>"] {
        let mut ed = Editor::from_text(text);
        ed.set_screen_size(20, 6);
        ed.feed(&format!("{is}llllj/ab<CR>jx"));
        let mut plain = Editor::from_text(text);
        plain.set_screen_size(20, 6);
        plain.feed("llllj/ab<CR>jx");
        assert_eq!(ed.buf().text(), plain.buf().text(), "{is}");
        assert_eq!(ed.cursor(), plain.cursor(), "{is}");
    }
}

/// The text cells of a row: the cells after the number column (`numberwidth`
/// defaults to 4), which must not carry a search highlight.
fn text_hl(ed: &Editor, r: usize) -> String {
    let all = row_hl(ed, r);
    assert!(
        !all[..4].contains('S') && !all[..4].contains('I'),
        "number column: {all}"
    );
    all[4..].to_string()
}

#[test]
fn number_column_offsets_hlsearch_and_incsearch_cells() {
    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set nu<CR>:set hls<CR>/foo<CR>");
    assert_eq!(text_hl(&ed, 0), exp("SSSNNNNNSSS", 16));
    assert_eq!(text_hl(&ed, 1), exp("NSSS", 16));

    let mut ed = Editor::from_text(FOO);
    ed.set_screen_size(20, 6);
    ed.feed(":set nu<CR>:set is<CR>/bar");
    assert_eq!(ed.mode(), Mode::CmdLine);
    assert_eq!(text_hl(&ed, 0), exp("NNNNIIINNNN", 16));
}

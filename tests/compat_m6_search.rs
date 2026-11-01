// SPDX-License-Identifier: Apache-2.0
//! 'hlsearch', 'incsearch' and `:nohlsearch` checked against real Vim. The
//! highlight itself is not visible to the harness (it compares text and
//! cursor), so these cases cover what the highlight does to the editing: the
//! option forms, `:noh` and the search that follows it, and an incsearch
//! prompt that is answered. The screen is covered in tests/search_highlight.rs.

mod common;
use common::{check, cross};

const TEXT: &str = "foo bar\nqux foo\nlast foo line\n";

#[test]
fn nohlsearch_does_not_move_the_cursor_or_change_text() {
    check(&[
        (TEXT, ":set hls<CR>/foo<CR>:noh<CR>x"),
        (TEXT, ":set hls<CR>/foo<CR>:nohl<CR>nx"),
        (TEXT, ":set hls<CR>/foo<CR>:nohlsearch<CR>Nx"),
        (TEXT, ":set hls<CR>/bar<CR>:noh<CR>:noh<CR>x"),
    ]);
}

#[test]
fn a_search_after_nohlsearch_moves_as_usual() {
    check(&[
        (TEXT, ":set hls<CR>/foo<CR>:noh<CR>nx"),
        (TEXT, ":set hls<CR>:noh<CR>/qux<CR>x"),
        (TEXT, ":set hls<CR>/foo<CR>:noh<CR>*x"),
    ]);
}

#[test]
fn hlsearch_option_forms_do_not_change_text() {
    check(&[
        (TEXT, ":set hls<CR>/qux<CR>x"),
        (TEXT, ":set nohls<CR>/qux<CR>x"),
        (TEXT, ":set invhls<CR>/qux<CR>x"),
        (TEXT, ":set invhls<CR>:set invhls<CR>/qux<CR>x"),
        (TEXT, ":set hls!<CR>:set hls&<CR>/qux<CR>x"),
        (TEXT, ":set hls?<CR>x"),
    ]);
}

#[test]
fn every_text_with_hlsearch_and_incsearch_keys() {
    let texts = [TEXT, "foo\n", "", "ab\n\ncd"];
    let keys = [":set hls<CR>/foo<CR>:noh<CR>x", ":set is hls<CR>/a<CR>x"];
    check(&cross(&texts, &keys));
}

#[test]
fn nohlsearch_keeps_the_desired_column() {
    check(&[("abcdef\nab\nabcdef", ":set hls<CR>llllj:noh<CR>jX")]);
}

#[test]
fn incsearch_forward_accept() {
    check(&[
        (TEXT, ":set is<CR>/qux<CR>x"),
        (TEXT, ":set is<CR>/foo<CR>x"),
        (TEXT, ":set is<CR>/line<CR>x"),
        (TEXT, ":set is<CR>/foo<CR>nx"),
    ]);
}

#[test]
fn incsearch_backward_accept() {
    check(&[
        (TEXT, ":set is<CR>G$?foo<CR>x"),
        (TEXT, ":set is<CR>G$?bar<CR>x"),
    ]);
}

#[test]
fn incsearch_with_no_match_leaves_the_cursor() {
    check(&[(TEXT, ":set is<CR>/zzz<CR>x")]);
}

#[test]
fn incsearch_with_an_operator() {
    check(&[
        (TEXT, ":set is<CR>d/qux<CR>"),
        (TEXT, ":set is<CR>wd/last<CR>"),
    ]);
}

#[test]
fn incsearch_and_hlsearch_together() {
    check(&[
        (TEXT, ":set is hls<CR>/foo<CR>nx"),
        (TEXT, ":set is hls<CR>/foo<CR>:noh<CR>/qux<CR>x"),
    ]);
}

// SPDX-License-Identifier: Apache-2.0
//! The `:s` `c` flag prompt where the differential harness cannot compare:
//! what is shown while it waits, a typed `<Esc>` (the harness runs Vim in
//! silent Ex mode, where `<Esc>` on an answer line is not a plain answer),
//! keys that are not answers, paste, and the single undo step.

use rvim::view::{Hl, frame};
use rvim::{Editor, Mode};

const TEXT: &str = "foo bar\nqux foo\nfoo";

#[test]
fn the_prompt_is_shown_on_the_match_it_asks_about() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.buf.text(), TEXT, "nothing is replaced before the answer");
    assert_eq!((ed.cursor().line, ed.cursor().col), (0, 0));
    // Wide enough that the ruler does not cut the prompt short.
    ed.set_screen_size(80, 4);
    let f = frame(&ed);
    assert!(
        f.status.starts_with("replace with X (y/n/a/q/l/^E/^Y)?"),
        "status was {:?}",
        f.status
    );
    // The match is highlighted; the rest of the line is not.
    assert!(f.hl[0][..3].iter().all(|h| *h == Hl::IncSearch));
    assert!(f.hl[0][3..].iter().all(|h| *h != Hl::IncSearch));
}

#[test]
fn the_highlight_moves_with_the_prompt() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>n");
    assert_eq!(ed.mode(), Mode::Confirm);
    // The next match is "foo" on line 2, after "qux ".
    assert_eq!((ed.cursor().line, ed.cursor().col), (1, 4));
    ed.set_screen_size(40, 4);
    let f = frame(&ed);
    assert!(f.hl[1][4..7].iter().all(|h| *h == Hl::IncSearch));
    assert!(f.hl[1][..4].iter().all(|h| *h != Hl::IncSearch));
    assert!(f.hl[0].iter().all(|h| *h != Hl::IncSearch));
}

#[test]
fn ctrl_e_and_ctrl_y_keep_the_prompt() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>");
    ed.feed("<C-e><C-y>");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.buf.text(), TEXT);
    ed.feed("y");
    assert_eq!(ed.buf.text(), "X bar\nqux foo\nfoo");
}

#[test]
fn keys_that_are_not_answers_ask_again() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>");
    ed.feed("jx<CR><Space>");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.buf.text(), TEXT);
    ed.feed("y");
    assert_eq!(ed.buf.text(), "X bar\nqux foo\nfoo");
}

#[test]
fn escape_quits_the_prompt_and_keeps_the_cursor_on_the_match() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>y");
    ed.feed("<Esc>");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.buf.text(), "X bar\nqux foo\nfoo");
    // The last prompt was for the match on line 2: quitting leaves the
    // cursor there, not at the first non-blank, as in Vim.
    assert_eq!((ed.cursor().line, ed.cursor().col), (1, 4));
    ed.feed("u");
    assert_eq!(ed.buf.text(), TEXT);
}

#[test]
fn ctrl_c_quits_like_q() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>y<C-c>");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.buf.text(), "X bar\nqux foo\nfoo");
}

#[test]
fn all_the_answers_make_one_undo_step() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>yyy");
    assert_eq!(ed.buf.text(), "X bar\nqux X\nX");
    assert_eq!(ed.mode(), Mode::Normal);
    ed.feed("u");
    assert_eq!(ed.buf.text(), TEXT);
    ed.feed("<C-r>");
    assert_eq!(ed.buf.text(), "X bar\nqux X\nX");
}

#[test]
fn an_undo_step_spans_the_prompts_with_other_keys_between() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>nyx!a");
    assert_eq!(ed.buf.text(), "foo bar\nqux X\nX");
    ed.feed("u");
    assert_eq!(ed.buf.text(), TEXT);
}

#[test]
fn pasted_text_is_not_an_answer() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed(":%s/foo/X/gc<CR>");
    ed.paste("yyy");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.buf.text(), TEXT);
    ed.feed("<Esc>");
    assert_eq!(ed.buf.text(), TEXT);
}

#[test]
fn a_bar_chain_runs_after_the_prompt_is_answered() {
    let mut ed = Editor::from_text("foo\nfoo bar");
    ed.feed(":%s/foo/X/gc|%s/bar/Y/<CR>");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.buf.text(), "foo\nfoo bar");
    // Declining the first match still leaves a prompt for the second.
    ed.feed("n");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.buf.text(), "foo\nfoo bar");
    ed.feed("a");
    assert_eq!(ed.mode(), Mode::Normal);
    // The first `:s` ends with the second line; then `%s/bar/Y/` runs.
    assert_eq!(ed.buf.text(), "foo\nX Y");
}

#[test]
fn global_with_confirm_visits_every_line_and_then_finishes() {
    let mut ed = Editor::from_text("a\nfoo\nfoo\nb");
    ed.feed(":g/foo/s/foo/X/c<CR>n");
    assert_eq!(ed.mode(), Mode::Confirm);
    ed.feed("y");
    // The last marked line was asked about and the `:g` is over.
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.buf.text(), "a\nfoo\nX\nb");
    ed.feed("u");
    assert_eq!(ed.buf.text(), "a\nfoo\nfoo\nb");
}

#[test]
fn each_line_of_a_global_asks_again_after_a() {
    // Each `:s` in `:g` reads its own flags, so `a` covers one line only.
    let mut ed = Editor::from_text("foo\nfoo\nfoo");
    ed.feed(":g/foo/s/foo/X/c<CR>a");
    assert_eq!(ed.mode(), Mode::Confirm);
    ed.feed("a");
    assert_eq!(ed.mode(), Mode::Confirm);
    ed.feed("a");
    assert_eq!(ed.mode(), Mode::Normal);
    assert_eq!(ed.buf.text(), "X\nX\nX");
}

/// `n` lines, each with a match: `foo 0` ... `foo {n-1}`.
fn numbered(n: usize) -> String {
    (0..n)
        .map(|i| format!("foo {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn ctrl_e_and_ctrl_y_scroll_the_window_and_keep_the_match() {
    let mut ed = Editor::from_text(&numbered(30));
    ed.feed(":set so=0<CR>:%s/foo/X/gc<CR>nnnn");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!((ed.cursor().line, ed.cursor().col), (4, 0));
    // Ten text rows: lines 0 to 9 are shown.
    ed.set_screen_size(40, 11);
    assert_eq!(ed.top_line(), 0);
    ed.feed("<C-e><C-e>");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.top_line(), 2);
    // The match does not move, and it is still the one highlighted.
    assert_eq!((ed.cursor().line, ed.cursor().col), (4, 0));
    assert!(ed.confirm_match().is_some_and(|(s, _)| s.line == 4));
    // A third scroll would put line 4 above the window: the window stops
    // with the match on its top line (scrolloff 0).
    ed.feed("<C-e><C-e><C-e>");
    assert_eq!(ed.top_line(), 4);
    assert_eq!((ed.cursor().line, ed.cursor().col), (4, 0));
    ed.feed("<C-y>");
    assert_eq!(ed.top_line(), 3);
    ed.feed("<C-y><C-y><C-y><C-y>");
    assert_eq!(ed.top_line(), 0, "CTRL-Y stops at the first line");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.buf.text(), numbered(30));
    // The prompt still answers the match it was showing.
    ed.feed("y");
    assert_eq!(ed.buf.text().lines().nth(4), Some("X 4"));
    assert_eq!(ed.buf.text().lines().nth(3), Some("foo 3"));
}

#[test]
fn ctrl_e_keeps_the_match_in_the_window_with_scrolloff() {
    let mut ed = Editor::from_text(&numbered(30));
    ed.feed(":set so=2<CR>:%s/foo/X/gc<CR>nnnn");
    assert_eq!(ed.mode(), Mode::Confirm);
    ed.set_screen_size(40, 11);
    ed.feed("<C-e><C-e><C-e><C-e><C-e><C-e>");
    // 'scrolloff' keeps two lines above the match, so the window's top
    // line is 2, not 6.
    assert_eq!(ed.top_line(), 2);
    assert_eq!((ed.cursor().line, ed.cursor().col), (4, 0));
    assert_eq!(ed.mode(), Mode::Confirm);
}

#[test]
fn ctrl_e_on_a_match_at_the_top_does_not_scroll_it_away() {
    let mut ed = Editor::from_text(&numbered(30));
    ed.feed(":set so=0<CR>:%s/foo/X/gc<CR>");
    ed.set_screen_size(40, 11);
    ed.feed("<C-e>");
    assert_eq!(ed.mode(), Mode::Confirm);
    assert_eq!(ed.top_line(), 0);
    assert_eq!((ed.cursor().line, ed.cursor().col), (0, 0));
    assert!(ed.confirm_match().is_some_and(|(s, _)| s.line == 0));
}

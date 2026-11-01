// SPDX-License-Identifier: Apache-2.0
//! The `/` and `?` prompt cases the differential harness cannot compare:
//! cancelling (the harness runs Vim in silent Ex mode, which types `<Esc>`
//! into a prompt), what is drawn while the prompt is open, and history
//! carried over many prompts. Editing keys are in tests/compat_m4_cmdline.rs.

use rvim::view::{Hl, frame};
use rvim::{Editor, Mode, VisualMode};

const TEXT: &str = "foo bar\nqux foo";

#[test]
fn backspace_on_an_empty_pattern_cancels() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed("/<BS>x");
    assert_eq!(ed.buf().text(), "oo bar\nqux foo");
    assert_eq!(ed.mode(), Mode::Normal);
}

#[test]
fn escape_and_ctrl_c_cancel_the_search_and_its_operator() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed("d/qux<Esc>");
    assert_eq!(ed.mode(), Mode::Normal);
    assert!(ed.pending_keys().is_empty());
    assert_eq!(ed.buf().text(), TEXT);
    ed.feed("x");
    assert_eq!(ed.buf().text(), "oo bar\nqux foo");

    let mut ed = Editor::from_text(TEXT);
    ed.feed("/qux<C-c>x");
    assert_eq!(ed.buf().text(), "oo bar\nqux foo");
    assert_eq!(ed.cursor().line, 0);
}

#[test]
fn escape_returns_to_visual_mode() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed("lv/qux<Esc>");
    assert_eq!(ed.mode(), Mode::Visual(VisualMode::Char));
    assert_eq!(ed.visual_start().map(|p| p.col), Some(1));
    ed.feed("d");
    assert_eq!(ed.buf().text(), "fo bar\nqux foo");
}

#[test]
fn the_visual_area_is_drawn_while_the_pattern_is_typed() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed("lV/qu");
    assert_eq!(ed.mode(), Mode::CmdLine);
    assert_eq!(ed.shown_visual(), Some(VisualMode::Line));
    assert_eq!(ed.visual_start().map(|p| p.line), Some(0));
    ed.set_screen_size(20, 4);
    let f = frame(&ed);
    assert_eq!(f.rows[0].trim_end(), "foo bar");
    // Linewise: the seven characters and the line break cell are in the area.
    assert!(f.hl[0][..8].iter().all(|h| *h == Hl::Visual));
    assert!(f.hl[0][8..].iter().all(|h| *h == Hl::Normal));
}

#[test]
fn prompt_shows_its_character_text_and_cursor() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed("d?fo<Left>");
    assert_eq!(ed.mode(), Mode::CmdLine);
    assert_eq!(ed.cmdline_prompt(), '?');
    assert_eq!(ed.cmdline(), "fo");
    assert_eq!(ed.cmdline_pos(), 1);
    ed.set_screen_size(20, 4);
    let f = frame(&ed);
    assert_eq!(f.status.trim_end(), "?fo");
    assert_eq!(f.cursor, (2, 3));
}

#[test]
fn history_keeps_200_patterns_like_vims_default() {
    let mut ed = Editor::from_text(TEXT);
    // 201 searches for patterns that do not match: the first one falls out.
    let searches: String = (0..=200).map(|i| format!("/a{i}<CR>")).collect();
    ed.feed(&searches);
    ed.feed("/");
    for _ in 0..200 {
        ed.feed("<Up>");
    }
    assert_eq!(ed.cmdline(), "a1");
    ed.feed("<Up>");
    assert_eq!(ed.cmdline(), "a1");
    assert!(ed.take_bell());
}

#[test]
fn a_recorded_search_replays_from_a_register() {
    let mut ed = Editor::from_text("a\nqux\nb\nqux");
    ed.feed("qa/qux<CR>xq");
    assert_eq!(ed.buf().text(), "a\nux\nb\nqux");
    // The `/` prompt was recorded as typed, so the replay searches again.
    ed.feed("@a");
    assert_eq!(ed.buf().text(), "a\nux\nb\nux");
}

#[test]
fn recalled_pattern_keeps_its_offset() {
    let mut ed = Editor::from_text(TEXT);
    ed.feed("/foo/e<CR>/<Up>");
    assert_eq!(ed.cmdline(), "foo/e");
}

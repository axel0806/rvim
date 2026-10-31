//! Bracketed paste is data: it must never run as commands, and it is one
//! undo step. (Vim's harness cannot send a paste, so these are unit tests.)

use rvim::Editor;

fn text(ed: &Editor) -> String {
    ed.buf.text()
}

#[test]
fn insert_mode_paste_is_literal_and_one_undo_step() {
    let mut ed = Editor::from_text("a\nb\n");
    ed.feed("A");
    ed.paste("x\r\ny\tz\x1bq");
    ed.feed("<Esc>");
    assert_eq!(text(&ed), "ax\ny\tzq\nb\n");
    ed.feed("u");
    assert_eq!(text(&ed), "a\nb\n");
}

#[test]
fn normal_mode_paste_does_not_run_commands() {
    let mut ed = Editor::from_text("one\ntwo\n");
    ed.paste("dd\n");
    assert_eq!(text(&ed), "dd\none\ntwo\n");
    ed.feed("u");
    assert_eq!(text(&ed), "one\ntwo\n");
}

#[test]
fn paste_during_pending_command_does_not_complete_it() {
    let mut ed = Editor::from_text("one\ntwo\n");
    ed.feed("d");
    ed.paste("d");
    assert_eq!(text(&ed), "done\ntwo\n");
}

#[test]
fn visual_paste_replaces_selection() {
    let mut ed = Editor::from_text("hello world\n");
    ed.feed("vee");
    ed.paste("bye");
    assert_eq!(text(&ed), "bye\n");
}

#[test]
fn cmdline_paste_drops_newlines() {
    let mut ed = Editor::from_text("a\nb\n");
    ed.feed(":");
    ed.paste("2\nd");
    assert_eq!(ed.cmdline(), "2d");
    ed.feed("<CR>");
    assert_eq!(text(&ed), "a\n");
}

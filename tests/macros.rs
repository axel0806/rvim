//! A failing command ends macro execution (Vim flushes the typeahead). The
//! differential harness cannot see this, see `compat_m4_macros.rs`.

use rvim::Editor;

#[test]
fn error_stops_the_macro_and_all_repeats() {
    let mut ed = Editor::from_text("a\nb\nc");
    ed.feed("qaA!<Esc>jq9@a");
    assert_eq!(ed.buf.text(), "a!\nb!\nc!");
    // The failing `j` on the last line ended all nine repeats.
    assert_eq!(ed.cursor().line, 2);
}

#[test]
fn failed_search_stops_the_macro() {
    let mut ed = Editor::from_text("a\nb\nb");
    ed.feed(":set nows<CR>qa/b<CR>A!<Esc>q3@a");
    // Without the abort, `A!` would run again after the failed search.
    assert_eq!(ed.buf.text(), "a\nb!\nb!");
}

#[test]
fn at_at_repeats_the_last_register() {
    let mut ed = Editor::from_text("abcdef");
    ed.feed("qaxq@a@@");
    assert_eq!(ed.buf.text(), "def");
    assert!(ed.register('a').is_some());
}

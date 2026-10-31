//! `:set` messages and errors (the Vim harness only sees buffer effects).

use rvim::Editor;

fn msg(keys: &str) -> String {
    let mut ed = Editor::from_text("x\n");
    ed.feed(keys);
    ed.message().map(|m| m.text.clone()).unwrap_or_default()
}

#[test]
fn set_queries() {
    assert_eq!(msg(":set sw?<CR>"), "  shiftwidth=8");
    assert_eq!(msg(":set sw=4 sw?<CR>"), "  shiftwidth=4");
    assert_eq!(msg(":set ai?<CR>"), "noautoindent");
    assert_eq!(msg(":set ai ai?<CR>"), "  autoindent");
    assert_eq!(msg(":set invet et?<CR>"), "  expandtab");
}

#[test]
fn set_errors() {
    assert!(msg(":set bogus<CR>").starts_with("E518"));
    assert!(msg(":set sw=x<CR>").starts_with("E521"));
    assert!(msg(":set sw=-1<CR>").starts_with("E487"));
    assert!(msg(":set ai=3<CR>").starts_with("E474"));
    assert!(msg(":set ts=4<CR>").starts_with("E474"));
}

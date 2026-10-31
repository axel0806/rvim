//! Milestone 4 input: Insert, function keys, Alt, Shift-Tab.

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &["foo bar baz\nqux foo quux\n\n  indented foo line\nlast foo"];

#[test]
fn extra_keys() {
    let keys = [
        "<Insert>x<Esc>",
        "w<Insert>xy<Esc>",
        "i<Insert>xy<Esc>",
        "i<Insert><Insert>xy<Esc>",
        "i<Insert>xy<Esc>u",
        "R<Insert>xy<Esc>",
        "<F5>",
        "<F5>x",
        ":<F5><CR>",
        "R<F5>",
        "3i<F5><Esc>",
        "ix<F5>y<Esc>",
        "d<F5>w",
        "ix<A-l>",
        "<A-j>",
        "<M-w>x",
        "ix<A-b>y",
        "i<S-Tab>x<Esc>",
        "<S-Tab>x",
        "d<Insert>w",
        "2<Insert>ab<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

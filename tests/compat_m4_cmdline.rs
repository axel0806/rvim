// SPDX-License-Identifier: Apache-2.0
//! Command-line editing keys and history, checked against Vim. The command
//! line effects show in the buffer, so commands are chosen to differ visibly.
//! (Vim runs in Ex mode here, where `<BS>` on an empty line does not cancel
//! and an empty `:<CR>` moves down a line; those cases are left out.)

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &["foo bar baz\nqux foo quux\n\n  indented foo line\nlast foo"];

#[test]
fn editing_keys() {
    let keys = [
        ":2d<Left><BS>3<CR>",
        ":d<Left>2<CR>",
        ":2<Left>d<CR>",
        ":3d<Home>2<CR>",
        ":3d<C-b>2<CR>",
        ":3d<C-b>2<C-e>y<CR>",
        ":3d<Home><End><CR>",
        ":32<Left><Left><Del>d<CR>",
        ":2 d<Left><Del><CR>",
        ":2d<Del>y<BS><CR>",
        ":2d x<C-w>y<CR>\"yp",
        ":2d y<C-w><CR>",
        ":foo bar<C-w>2d<CR>",
        ":2d<Home>zzz<C-u>3<CR>",
        ":zzz<Home><Right><C-u>2d<CR>",
        ":zzz<C-u>2d<CR>",
        ":<Left>2d<CR>",
        ":2d<Right><Right>3<CR>",
        ":s/foo/X/<Left><Left>Y<CR>",
        ":s/foo/X/<Left><Left><Left><Left>a<CR>",
        ":s/o/0/g<Home><Right><Right>%<CR>",
        ":3<C-v>d<CR>",
        ":2<C-v><Esc><Left>d<CR>",
        "yiw:s/<C-r>\"/Y/<CR>",
        "yiw:s/<C-r>0/Y/<CR>",
        "\"ayiw:s/<C-r>a/Y/<CR>",
        "w:s/<C-r><C-w>/Z/<CR>",
        "wl:s/<C-r><C-w>/Z/<CR>",
        ":s/<C-r><C-l>/Z/<CR>",
        "/o<CR>:s/<C-r>//Z/<CR>",
        "yy:2d<C-r>\"<CR>",
        "yy:<C-r>\"<BS>d<CR>",
        ":2<C-r>a<CR>",
        ":2<C-r><C-r>\"<CR>",
        ":2<C-r><C-w><CR>",
        ":2d|3d<CR>:1<CR>:<Up><CR>",
        ":2d<CR>:<Up><CR>",
        ":2d<CR>:3d<CR>:<Up><Up><CR>",
        ":2d<CR>:3d<CR>:<Up><Up><Down><CR>",
        ":2d<CR>:3d<CR>:<Up><Up><Up><CR>",
        ":2d<CR>:3d<CR>:<Down><CR>",
        ":3d<CR>:2d<CR>:3<Up><CR>",
        ":3d<CR>:2d<CR>:2<Up><Down>1<CR>",
        ":s/o/0/<CR>:2<CR>:s<Up><CR>",
        ":2d<CR>:3d<CR>:<C-p><C-p><CR>",
        ":2d<CR>:3d<CR>:<C-p><C-n><CR>",
        ":2d<CR>:3d<CR>:3<C-p><CR>",
        ":2d<CR>:2d<CR>:<Up><Up><CR>",
        ":2d<CR>:3d<CR>:2d<CR>:<Up><Up><CR>",
        ":2d<CR>:<Up><Left><Left>3<CR>",
        ":2d<CR>:<Up><BS>3<CR>",
    ];
    check(&cross(TEXTS, &keys));
}

/// `/` and `?` prompts: the same editing keys, `<C-r>` and history as `:`.
/// Cancelling with `<Esc>` is not compared: Vim's silent Ex mode types the
/// `<Esc>` into the prompt, so those cases are in tests/search_prompt.rs.
#[test]
fn search_prompt_keys() {
    let keys = [
        "/fo<Left>o<CR>",
        "/qx<Left><Left><Right>u<CR>",
        "/fooo<Left><Del><CR>",
        "/foo<Home>x<CR>",
        "/o<C-b>fo<CR>",
        "/foo baz<C-w>quux<CR>",
        "/foo bar<C-u>qux<CR>",
        "/fooo<Left><Left>x<C-u>qux<CR>",
        "/qu<BS><BS>fo<CR>",
        "2/foo<CR>",
        "G?fo<Left>o<CR>",
        "d?foo<CR>",
        "c/qux<CR>X<Esc>",
        "y/qux<CR>P",
        "/fxo<Left><BS>o<CR>",
        "ggd/fxo<Left><BS>o<CR>.",
        "f/x<CR>",
        "r/x",
        "g??",
        "yiw/<C-r>\"<CR>",
        "\"ayiw/<C-r>a<CR>",
        "yiw/<C-r>0<CR>",
        "yiw/<C-r><C-r>\"<CR>",
        "yiw/<C-r>\"<Left>x<CR>",
        "w/<C-r><C-w><CR>",
        "w/<C-r><C-a><CR>",
        "/<C-r><C-l><CR>",
        "/qux<CR>gg/<C-r>/<CR>",
        "/qux<CR>gg?<C-r>/<CR>",
        "yiwwd/<C-r>\"<CR>",
        "v/qux<CR>d",
        "vl/qux<C-u>foo<CR>d",
        "V/qux<CR>d",
        "v/qux<CR>/foo<CR>x",
        "/qux<CR>/<Up><CR>",
        "/foo<CR>/qux<CR>/<Up><Up><CR>",
        "/foo<CR>/qux<CR>/<Up><Up><Up><CR>",
        "/foo<CR>/qux<CR>/<Up><Up><Down><CR>",
        "/foo<CR>/qux<CR>/<C-p><C-p><C-n><CR>",
        "/foo<CR>/qux<CR>/f<Up><CR>",
        "/foo<CR>/qux<CR>/z<Up><CR>",
        "/foo<CR>/qux<CR>/fo<Up><Down><CR>",
        "/foo<CR>/qux<CR>/<Up><BS>x<CR>",
        "/foo<CR>/qux<CR>?<Up><CR>",
        "/foo<CR>/qux<CR>gg?<Up><CR>",
        "/foo<CR>/qux<CR>ggd/<Up><CR>",
        "/qux<CR>ggd/<Up><CR>.",
        "/qux<CR>ggd/<Up><CR>j.",
        "/qux<CR>:s/o/0/<CR>/<Up><CR>",
        "/qux<CR>:s/o/0/<CR>:<Up><CR>",
        ":s/o/0/<CR>/<Up><CR>",
        "/qux<CR>/qu<Down><CR>",
        "/foo/e<CR>n",
        "*/<Up><CR>",
        "#/<Up><CR>",
        "g*/<Up><CR>",
        ":g/qux/d<CR>/<Up><CR>",
        "/qux<CR>:/foo/d<CR>/<Up><CR>",
        "/foo<CR>/qux<CR>/<Up><Up><Up><Up><CR>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn ampersand() {
    let keys = [
        "&",
        "g&",
        ":s/o/0/<CR>j&",
        ":s/o/0/g<CR>j&",
        ":s/o/0/g<CR>jg&",
        ":s/o/0/<CR>jg&",
        ":s/O/0/gi<CR>g&",
        ":s/o/0/<CR>j&u",
        ":s/o/0/<CR>jj&j&",
        ":s/o/0/<CR>jg&u",
        ":s/foo/X/<CR>jg&",
        ":s/foo/X/g<CR>G&",
        ":%s/o/0/<CR>&",
        "/foo<CR>:s//X/<CR>jg&",
    ];
    check(&cross(TEXTS, &keys));
}

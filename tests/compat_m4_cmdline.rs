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

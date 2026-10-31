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

const INDENTED: &[&str] = &[
    "foo bar baz\n\tqux foo quux\n  \n    indented foo line\nlast foo",
    "if (a) {\n\treturn 1;\n}\n  x\n\t\ty z",
];

#[test]
fn autoindent() {
    let keys = [
        ":set ai<CR>jofoo<Esc>",
        ":set ai<CR>jOfoo<Esc>",
        ":set ai<CR>jo<Esc>",
        ":set ai<CR>jO<Esc>",
        ":set ai<CR>jo<CR><Esc>",
        ":set ai<CR>jo<CR>x<Esc>",
        ":set ai<CR>jA<CR>x<Esc>",
        ":set ai<CR>jA<CR><Esc>",
        ":set ai<CR>jAx<CR>y<Esc>",
        ":set ai<CR>jwi<CR>x<Esc>",
        ":set ai<CR>jI<CR>x<Esc>",
        ":set ai<CR>j0i<CR>x<Esc>",
        ":set ai<CR>jwi  <CR>x<Esc>",
        ":set ai<CR>3jA<CR>x<Esc>",
        ":set ai<CR>jccfoo<Esc>",
        ":set ai<CR>jcc<Esc>",
        ":set ai<CR>jSx<Esc>",
        ":set ai<CR>2jccx<Esc>",
        ":set ai<CR>jjccx<Esc>",
        ":set ai<CR>j3ofoo<Esc>",
        ":set ai<CR>j3Ofoo<Esc>",
        ":set ai<CR>jofoo<Esc>u",
        ":set ai<CR>jA<CR>x<Esc>u",
        ":set ai<CR>jofoo<Esc>j.",
        ":set ai<CR>jA<CR>x<Esc>j.",
        ":set ai<CR>jo<Esc>j.",
        ":set ai<CR>jofoo<BS><BS><BS><BS><BS><Esc>",
        ":set ai<CR>jo<BS>x<Esc>",
        ":set ai<CR>jo<Left>x<Esc>",
        ":set ai<CR>jo<Esc>ix<Esc>",
        ":set noai<CR>jofoo<Esc>",
        ":set ai<CR>:set noai<CR>jofoo<Esc>",
        ":set ai!<CR>jofoo<Esc>",
        ":set invai<CR>jofoo<Esc>",
        ":set autoindent<CR>jofoo<Esc>",
        ":set ai<CR>jRx<CR>y<Esc>",
        ":set ai<CR>Gox<Esc>",
        ":set ai<CR>GOx<Esc>",
    ];
    check(&cross(INDENTED, &keys));
}

#[test]
fn shiftwidth_and_expandtab() {
    let keys = [
        ":set sw=4<CR>>>",
        ":set sw=4<CR>>j",
        ":set sw=4<CR>jj>>",
        ":set sw=4<CR>j<<",
        ":set sw=4<CR>j2<<",
        ":set sw=4<CR>3>>",
        ":set sw=2<CR>>>>>",
        ":set sw=2<CR>j>>>>",
        ":set sw=0<CR>>>",
        ":set sw=3<CR>Vj>",
        ":set sw=3<CR>Vj2>",
        ":set sw=3<CR><C-v>j>",
        ":set sw=3<CR>w<C-v>j>",
        ":set sw=3<CR>j<C-v>jj<",
        ":set sw=4 et<CR>>>",
        ":set sw=4 et<CR>j>>",
        ":set sw=4 et<CR>jj>>",
        ":set sw=4 et<CR>3>>",
        ":set sw=4 et<CR>>G",
        ":set sw=4 et<CR>j<<",
        ":set sw=4 et<CR>Vj>",
        ":set sw=4 et<CR>w<C-v>j>",
        ":set sw=4 et<CR>j<C-v>jj<",
        ":set et<CR>>>",
        ":set et<CR>j>>",
        ":set et<CR>i<Tab><Esc>",
        ":set et<CR>wi<Tab><Esc>",
        ":set et<CR>A<Tab>x<Esc>",
        ":set et<CR>jA<Tab><Tab>x<Esc>",
        ":set et<CR>3i<Tab><Esc>",
        ":set et<CR>jI<Tab>x<Esc>",
        ":set et<CR>jR<Tab><Esc>",
        ":set et<CR>R<Tab><Esc>",
        ":set et<CR>wR<Tab>x<BS><BS><Esc>",
        ":set et sw=4<CR>:2,3><CR>",
        ":set noet<CR>i<Tab><Esc>",
        ":set sw=4<CR>:set sw<CR>>>",
        ":set sw+=2<CR>>>",
        ":set sw=4<CR>:set sw-=1<CR>>>",
        ":set sw=2<CR>:set sw^=2<CR>>>",
        ":set sw=4<CR>:set sw&<CR>>>",
        ":set sw=4<CR>:set shiftwidth&<CR>>>",
        ":set sw=x<CR>>>",
        ":set sw=-1<CR>>>",
        ":set bogus<CR>>>",
        ":set nobogus<CR>>>",
        ":set ai?<CR>>>",
        ":set noet et?<CR>>>",
        ":set et!<CR>>>",
    ];
    check(&cross(INDENTED, &keys));
}

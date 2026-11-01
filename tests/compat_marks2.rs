// SPDX-License-Identifier: Apache-2.0
//! The marks that are not lowercase, checked against Vim: `'[` `']` (the
//! last change or yank), `'<` `'>` (the last Visual area) and `'"`, as
//! motions, operator targets and Ex addresses; what sets `'[` and `']` for
//! each command; `m[`, `m]`, `m<`, `m>`; and `:delmarks`. The marks are
//! read through their motions, so the cursor the harness compares shows
//! where each one is.

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &[
    "abc def\nghi jkl\nmno",
    "a\nb\nc\nd\ne",
    "foo bar\n\tbaz qux\nfoo\n  quux foo",
    "abcdef\nab\nabcdef\nabcdef",
];

#[test]
fn motions_to_change_and_yank_marks() {
    let keys = [
        "yw`[",
        "yw`]",
        "yy`]",
        "jyy`[",
        "jyyG`]",
        "vjy`]",
        "Vjy`]",
        "Vjy`[",
        "wyiw`[",
        "wyiw`]",
        "y$`]",
        "ywP`[",
        "ywP`]",
        "ywp`[",
        "ywp`]",
        "yyp`[",
        "yyp`]",
        "yyP`]",
        "yyjgp`]",
        "yyjgP`[",
        "ylgp`]",
        "yiwjwp`[",
        "yiwjwp`]",
        "yiwjwgp`]",
        "wdw`[",
        "wdw`]",
        "dd`[",
        "jdd`]",
        "jdd`[",
        "wcwX<Esc>`[",
        "wcwX<Esc>`]",
        "wiX<Esc>`[",
        "wiX<Esc>`]",
        "aX<Esc>`]",
        "oX<Esc>`[",
        "OX<Esc>`]",
        "2iX<Esc>`]",
        "ylP`]",
        "x`]",
        "xp`]",
        "ddp`[",
        "jxxu`[",
        "jxxu`]",
        "xjxuu`]",
        "jxxuu<C-r>`]",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn motions_to_shift_join_case_and_ex_marks() {
    let keys = [
        ">>`[",
        ">>`]",
        "j>j`]",
        "Vj>`]",
        "2>>`]",
        "J`[",
        "J`]",
        "3J`]",
        "3J`[",
        "jJ`]",
        "~`]",
        "3~`]",
        "wg~w`[",
        "wgUiw`]",
        "wguw`[",
        "wguw`]",
        "wg?w`]",
        ":normal Ax<CR>`[",
        ":normal Ax<CR>`]",
        ":normal 2>>`]<CR>",
        "gUU`]",
        "gUU`[",
        ":2,3s/$/!/<CR>`[",
        ":2,3s/$/!/<CR>`]",
        ":%s/o/0/g<CR>`]",
        ":1,2d<CR>`[",
        ":2m0<CR>`[",
        ":2m0<CR>`]",
        ":1t$<CR>`]",
        "yy:pu<CR>`]",
        ":2y<CR>`]",
        ":2,3j<CR>`]",
        "ddjpu`[",
        "ddjpu`]",
        "yyjpu`]",
        "wdwu`[",
        "wdwu`]",
        "wdwu<C-r>`]",
        "xjxu`[",
        "xjxuu`]",
        "xjxuu<C-r>`]",
        "jcwXY<Esc>uu`]",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn motions_to_visual_marks() {
    let keys = [
        "vjy`<",
        "vjy`>",
        "vjy'>",
        "vjy'<",
        "wvey`<",
        "wvey`>",
        "Vjy`>",
        "Vjy`<",
        "Vjy'>",
        "wvjd`<",
        "wvlbd`>",
        "vj<Esc>`>",
        "vj<Esc>`<",
        "jvj<Esc>gg`>",
        "jVk<Esc>gg`<",
        "vjy`<d`>",
        "vjy`>ix<Esc>",
        "jVjy`>ix<Esc>",
        "jlvjy`<ix<Esc>",
        "Vjy``",
        "lllvjyG`<ix<Esc>",
        "vjy`<``",
        "vjy`>`<",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn operators_take_the_special_marks() {
    let keys = [
        "jyyjd'[",
        "jyyjd`]",
        "wyw$d`]",
        "wyw$d`[",
        "jyyG`]",
        "jyy`]d'[",
        "wvjy`>d`<",
        "jVjy<Esc>gg`>d'>",
        "jVjy<Esc>ggd'>",
        "jyw$d`]",
        "lvly`>d`<",
        "jcwX<Esc>gg`[d`]",
        "jlwiX<Esc>ggd`]",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn insert_sets_start_and_end_marks() {
    let keys = [
        "ihello<Esc>`[",
        "ihello<Esc>`]",
        "Ahey<Esc>`[",
        "Ahey<Esc>`]",
        "2ihi<Esc>`]",
        "2ihi<Esc>`[",
        "jwiXYZ<Esc>`[",
        "jwiXYZ<Esc>`]",
        "jwcwXY<Esc>`[",
        "jwcwXY<Esc>`]",
        "jwoNEW<Esc>`[",
        "jwoNEW<Esc>`]",
        "jwOnew<Esc>`[",
        "jwOnew<Esc>`]",
        "ojk<CR>x<Esc>`[",
        "ojk<CR>x<Esc>`]",
        "iX<CR>Y<Esc>`]",
        "iX<CR>Y<Esc>`[",
        "ihi<Esc>`]iX<Esc>",
        "i<Esc>`]",
        "i<Esc>`[",
        "llaX<Left>Y<Esc>`]",
        "llaX<Left>Y<Esc>`[",
        "llaX<Left>Y<Esc>`[ix<Esc>",
        "AXYZ<BS><BS><Esc>`[",
        "AXYZ<BS><BS><Esc>`]",
        "jwiX<BS><BS><Esc>`[",
        "jwiX<BS><BS><Esc>`]",
        "A<Esc>`]",
        "Ahey<Esc>uu`]",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn undo_and_redo_set_marks() {
    let keys = [
        "xjxu`[",
        "xjxu`]",
        "ddu`[",
        "ddu`]",
        "jddu`]",
        "jddu`[",
        "ddjpuu`[",
        "ddjpuu`]",
        "ddjpuu<C-r>`]",
        "jxxu<C-r>`[",
        "Ahey<Esc>u`]",
        "wdwu`[",
        "2ihi<Esc>u`]",
        "jJu`[",
        "jJu`]",
        ":2,3s/$/!/<CR>u`[",
        ":2,3s/$/!/<CR>u`]",
        "jyyuu`[",
        "jxuu`[",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn marks_are_set_by_motions_and_keep_the_context_mark() {
    let keys = [
        "jyy`[``ix<Esc>",
        "jlyy`]``ix<Esc>",
        "llljyw`]``ix<Esc>",
        "lllljyw`[``ix<Esc>",
        "jlwiX<Esc>`[``ix<Esc>",
        "jj`\"ix<Esc>",
        "jj`\"``ix<Esc>",
        "jj'\"ix<Esc>",
        "jm[`[ix<Esc>",
        "jm]gg`]ix<Esc>",
        "jm]gg']ix<Esc>",
        "llm[G`[ix<Esc>",
        "jlvjy`[",
        "jllm<gg`<ix<Esc>",
        "jllm>gg`>ix<Esc>",
        "jlvjm>gg`>ix<Esc>",
        "jlvjm<gg`<ix<Esc>",
        "jjmakkm]`]ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn desired_column_after_special_marks() {
    // A mark motion resets the desired column, and so does a failed one; a
    // following `j` starts from the column the mark moved to.
    let keys = [
        "lllljyw`[j",
        "lllljyw`]j",
        "llllj`[j",
        "llllj`]j",
        "llllj'[j",
        "llllj']j",
        "llllj`<j",
        "lllljvly<Esc>`<j",
        "lllljVy`>j",
        "llllj`>j",
        "lllljyw`[jix<Esc>",
        "llllj`\"j",
        "lllljm[`[j",
        "lllljm<`<j",
        // A setting of a mark keeps the desired column.
        "lllljm[jix<Esc>",
        "lllljm]jjix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn ex_addresses_with_the_special_marks() {
    let keys = [
        "jyy:'[,']d<CR>ix<Esc>",
        "jVjy<Esc>:'[,']d<CR>ix<Esc>",
        "jVjy<Esc>:'[,']s/$/!/<CR>ix<Esc>",
        "jVjy<Esc>:'[,']m0<CR>ix<Esc>",
        "jVjy<Esc>:'[,']t$<CR>ix<Esc>",
        "jVjy<Esc>:'[d<CR>ix<Esc>",
        "jVjy<Esc>:']d<CR>ix<Esc>",
        "jVjy<Esc>:'[+1d<CR>ix<Esc>",
        "jVjy<Esc>gg:'<,'>d<CR>ix<Esc>",
        "vjy<Esc>:'<,'>d<CR>ix<Esc>",
        "vjy<Esc>:'>d<CR>ix<Esc>",
        "vj<Esc>:'<,'>s/$/!/<CR>ix<Esc>",
        ":'[d<CR>ix<Esc>",
        ":'<,'>d<CR>ix<Esc>",
        ":'[,']d<CR>ix<Esc>",
        "'[ix<Esc>",
        "ihi<Esc>:'[,']d<CR>ix<Esc>",
        "jwiX<Esc>gg:'[,']d<CR>ix<Esc>",
        "xj:'[d<CR>ix<Esc>",
        "jyy:'\"d<CR>ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn delmarks_and_marks_with_errors() {
    let keys = [
        "jyy:delm [<CR>`[ix<Esc>",
        "jyy:delm ]<CR>`]ix<Esc>",
        "jyy:delm!<CR>`[ix<Esc>",
        "jyy:delm!<CR>`]ix<Esc>",
        "jmajmbjmc:delm a-c<CR>'bix<Esc>",
        "jmajmbjmc:delm a-b<CR>'cix<Esc>",
        "jmajmbjmc:delm a b<CR>'cix<Esc>",
        "jmajmbjmc:delm ab<CR>'cix<Esc>",
        "jmajmbjmc:delm a -b<CR>'bix<Esc>",
        "jmajmbjmc:delm a--<CR>'aix<Esc>",
        "jmajmbjmc:delm b-a<CR>'aix<Esc>",
        "jmajmbjmc:delm a-Z<CR>'aix<Esc>",
        "jmajmbjmc:delm!<CR>'aix<Esc>",
        "jmajmbjmc:delm!<CR>'cix<Esc>",
        "jmajmbjmc:delm!<CR>ggjix<Esc>",
        "jmagg:delm a<CR>'aix<Esc>",
        "jmagg:delm x<CR>'aix<Esc>",
        "jmagg:delm 1<CR>'aix<Esc>",
        "jmagg:delm A-Z<CR>'aix<Esc>",
        "jmagg:delm 0-9<CR>'aix<Esc>",
        "jmagg:delm<CR>'aix<Esc>",
        "jmagg:delm!a<CR>'aix<Esc>",
        "jmagg:delm! a<CR>'aix<Esc>",
        "jmagg:delm *<CR>'aix<Esc>",
        "jmagg:delm ' <CR>'aix<Esc>",
        "jmagg:delm a'<CR>'aix<Esc>",
        "jjwm[G:delm [<CR>`[ix<Esc>",
        "jjwyy:delm<CR>`]ix<Esc>",
        "jlmagg:marks a<CR>'aix<Esc>",
        "jlmagg:marks x<CR>'aix<Esc>",
        "jlmagg:marks<CR>'aix<Esc>",
        "jlmagg:marks!<CR>'aix<Esc>",
        "jlmagg:marks <'<CR>'aix<Esc>",
        "jlmagg:marks a'<CR>'aix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

/// Visual mode on a TAB: the desired column after leaving Visual is the
/// Normal-mode one (the end of the TAB), while a vertical motion inside
/// Visual uses the column stored before it.
#[test]
fn visual_on_a_tab_keeps_the_desired_column() {
    const TAB_TEXTS: &[&str] = &[
        "\tabc\n12345678901234567890",
        "ab\tcd\n12345678901234567890\n\tq",
        "\tabc\tdef\n1234\n\t",
    ];
    let keys = [
        "vk<Esc>jix<Esc>",
        "V<Esc>jix<Esc>",
        "llvoj<Esc>ix<Esc>",
        "vjd",
        "jvkd",
        "vjj<Esc>jix<Esc>",
        "vjy`>ix<Esc>",
        "ll<C-v>jjy`>ix<Esc>",
        "vj<Esc>jix<Esc>",
        "lvj<Esc>jix<Esc>",
        "vjjj<Esc>kix<Esc>",
        "vl<Esc>jix<Esc>",
        "v$<Esc>jix<Esc>",
        "vlld3vd",
        "lv$<Esc>jix<Esc>",
    ];
    check(&cross(TAB_TEXTS, &keys));
}

/// Known difference: Vim moves the ends of a Visual area with the lines an
/// edit changes (mark_adjust()); rvim keeps them where they were.
#[test]
#[ignore = "known difference: Visual area does not follow line edits (PLAN.md, Known differences)"]
fn visual_marks_after_a_line_edit() {
    let keys = ["wvjd`>", "wvjd`<"];
    check(&cross(TEXTS, &keys));
}

/// Known difference: after `<C-r>` of a linewise `p`, Vim's `']` is one
/// line further than rvim's (u_undoredo() of the redone entry).
#[test]
#[ignore = "known difference: ']' after redo of a linewise put (PLAN.md, Known differences)"]
fn redo_of_a_linewise_put_sets_end_mark() {
    let keys = ["ddjpu<C-r>`]"];
    check(&cross(TEXTS, &keys));
}

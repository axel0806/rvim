// SPDX-License-Identifier: Apache-2.0
//! Uppercase (file) marks checked against Vim: `mA`-`mZ`, `'A` and `` `A ``
//! (also as operator motions and Visual ends), `:k`/`:mark`, `'A` as an Ex
//! address, how the marks follow line and column changes and `:m`, and
//! `:delmarks` with `A-Z`. Undo does not restore them, as in Vim.

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &[
    "abcdef\nab\n  abc\nxyz\nlast line",
    "one\ntwo\nthree\nfour",
    "foo bar\n\tbaz qux\nfoo\n  quux foo",
];

#[test]
fn set_and_jump() {
    let keys = [
        "jmAgg'Aix<Esc>",
        "jllmAgg`Aix<Esc>",
        "lllmAj0`Ax",
        "lllmAj0`Aix<Esc>",
        "jmAjmBgg'Bix<Esc>'Aix<Esc>",
        "jmAggO<Esc>'Aix<Esc>",
        "jjmAggOx<Esc>'AD",
        "jjmAgg:'Aix<Esc>",
        "jmAG'Aix<Esc>",
        "jjmAgg3`Aix<Esc>",
        "lll`Ax",
        "lll'Ax",
        "'Ax",
        "gg`Qx",
        "gg'Qx",
        "gg'*x",
        "gg`1x",
        // The desired column: a jump resets it, a failed jump moves nothing.
        "lllljmA'Aj",
        "lllljmA`Aj",
        "lllljmakk`Aj",
        "llllj:kA<CR>j",
        "llllj:kA<CR>k",
        // Setting a file mark keeps the desired column.
        "lllljmAjix<Esc>",
        "lllljmAjjix<Esc>",
        "lllljmAkkjix<Esc>",
        "jjlmAkgg`Ax",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn operators_take_file_marks_as_motions() {
    let keys = [
        "lllmAj0d`A",
        "lllmAj0d'A",
        "lllmAj0y`AP",
        "jmAdd'Aix<Esc>",
        "jmAdd`Aix<Esc>",
        "jjmAggdd'Aix<Esc>",
        "jjmAggdd`Aix<Esc>",
        "jjmAgg3d'Aix<Esc>",
        "jjmAgg3d`Aix<Esc>",
        "jjmAgg2d'Aix<Esc>",
        "jjmAgg2d`Aix<Esc>",
        "jjmAGmBggd'B",
        "jjmAgg:'A,'Ad<CR>",
        "jjmAgg:'A,$d<CR>",
        "jjmAgg:.,'Ad<CR>",
        "jjmAgg:'Ad<CR>",
        "jjjmAgg:'Ad<CR>x",
        "jjmAjj:'Ad<CR>",
        "jjmAgg:'Ay<CR>P",
        "jjmAk:'A<CR>x",
        "jjmAVj'Ax",
        "jjmAggVj`Ad",
        "jjmAggvj'Ad",
        "jjlmAggvj`Ad",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn marks_follow_line_edits_and_undo_does_not_restore_them() {
    let keys = [
        // Deleting the marked line moves the mark to the line after it,
        // and undo does not bring it back.
        "jjmAdd'Aix<Esc>",
        "jjmAddu'Aix<Esc>",
        "jjmAddu`Ax",
        // Deleting a line above, and undoing it, moves the mark back.
        "jjmAggdd'Aix<Esc>",
        "jjmAggddu'Aix<Esc>",
        "jjlmAggdd`Ax",
        "jjmA:1,2d<CR>'Aix<Esc>",
        "jjmA:1,2d<CR>u'Aix<Esc>",
        "jjmA:1,'Ad<CR>",
        "jjmAGdd'Aix<Esc>",
        "jjmAG'Aix<Esc>",
        "jjmAo x<Esc>'Aix<Esc>",
        "jjmAO x<Esc>'Aix<Esc>",
        "jjmAkJ'Aix<Esc>",
        "jjmAkJ`Ax",
        "jjmAyyP'Aix<Esc>",
        "jjmA\"ayy'Aix<Esc>",
        "jjmAcc<Esc>'Aix<Esc>",
        "jjmAxu'Aix<Esc>",
        "jjmAx'Aix<Esc>",
        "jjmA:2,3m0<CR>'Aix<Esc>",
        "jjmA:2,3m0<CR>`Ax",
        "jjmAdd:'Ax",
        "jjmAdk'Ax",
        "jjmAggu'Ax",
        // A mark that moved past the end: E19, the cursor stays.
        "jjjmAdd'Ax",
        "jjjmAdd`Ax",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn set_with_ex_and_delete() {
    let keys = [
        "jjmA:delmarks A<CR>'Aix<Esc>",
        "jjmA:delmarks!<CR>'Aix<Esc>",
        "jjmA:delmarks A-C<CR>'Aix<Esc>",
        "jjmA:delmarks B-C<CR>'Aix<Esc>",
        "jjmA:k B<CR>'Bix<Esc>",
        "jjmA:k B<CR>:delmarks A-B<CR>'Bix<Esc>",
        "jjmA:marks A<CR>",
        "jjmAgg:mark C<CR>`Cx",
        "jjmAmBgg:delmarks!<CR>'Bx",
        "jjmAmBgg:delmarks!<CR>'Ax",
        "jjmAggmA'Ax",
        "jjmAgg:'A<CR>ix<Esc>",
        "ggjmZggjmA:delmarks Z<CR>'Ax",
        "ggjjmAgg:marks ZA<CR>'Zx",
        "jmAggjmBgg:'A,'Bd<CR>",
        "jmAggjmBGd'B",
        "gg\"Ap",
    ];
    check(&cross(TEXTS, &keys));
}

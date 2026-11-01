// SPDX-License-Identifier: Apache-2.0
//! `:checktime` and 'autoread' checked against Vim, where the harness can
//! express them. The harness buffers have no file, so nothing ever changes on
//! disk: these cases show that the commands leave the text and the cursor as
//! Vim leaves them, and that setting 'autoread' changes nothing by itself.
//! A changed file is not in the harness (it cannot write files between keys);
//! those cases are unit tests in `src/ex.rs` and `src/editor.rs`, and the
//! placement of the cursor after a reload is in PLAN.md's Known differences.

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &["one\ntwo\nthree", "a\n  b\n\tc\nlast line", "x"];

#[test]
fn checktime_with_no_file_changes_nothing() {
    let keys = [
        ":checktime<CR>",
        "jl:checktime<CR>x",
        ":checkt<CR>ix<Esc>",
        "Gx:checktime<CR>u",
        ":set autoread<CR>:checktime<CR>ix<Esc>",
        ":set ar<CR>jj:checkt<CR>dd",
        ":set noautoread<CR>:checktime<CR>G",
        "3j:checktime<CR>kx",
        "lll:checktime<CR>a<Esc>",
        ":set ar<CR>:checktime<CR>:set noar<CR>:checktime<CR>J",
        ":checktime 1<CR>ix<Esc>",
        "jj:checktime 2<CR>dd",
        ":checktime %<CR>ix<Esc>",
        ":checktime nosuch<CR>ix<Esc>",
        ":checktime 0<CR>ix<Esc>",
        ":set ar<CR>:checktime 1<CR>J",
    ];
    check(&cross(TEXTS, &keys));
}

/// With the agent opt-in off (the default), a modified buffer with no file
/// change keeps Vim's behaviour around `:w` and `:checktime`. The harness
/// buffers have no file name, so `:w` is the no-name case; the checks that
/// need a changed file are unit tests in `src/conflict.rs`.
#[test]
fn w_and_checktime_around_edits_and_undo_without_a_change() {
    let keys = [
        "ix<Esc>:checktime<CR>u<C-r>",
        "jx:checktime<CR>ddu",
        ":set ar<CR>ix<Esc>:checktime<CR>u",
        "Ax<Esc>:checktime<CR>:set ar<CR>:checktime<CR>",
        "ix<Esc>:w<CR>u",
        "jx:checktime<CR>:w<CR>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn autoread_on_does_not_change_other_keys() {
    let keys = [
        ":set autoread<CR>jjdd",
        ":set ar<CR>wdw",
        ":set ar<CR>ddu<C-r>",
        ":set ar<CR>ma:checktime<CR>'ax",
        ":set ar<CR>yyp:checktime<CR>",
    ];
    check(&cross(TEXTS, &keys));
}

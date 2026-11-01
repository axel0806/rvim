// SPDX-License-Identifier: Apache-2.0
//! Options that change tab handling: 'tabstop', 'softtabstop', 'smarttab' and
//! 'shiftround'. Each option is set with `:set` in the case's keys, which
//! runs in both rvim and Vim; the harness resets them before every case.

mod common;
use common::{check, cross};

/// Text with tabs and wide characters, so display columns differ from
/// character columns.
const TEXTS: &[&str] = &[
    "foo\tbar\tbaz\nqux\n\tindented\tfoo\n  x\ty",
    "漢字 wide\tend\n語\tx漢\n\tlast 漢",
];

/// Indented text for shifting and Insert-mode Tab.
const INDENT: &[&str] = &[
    "a\n\nfoo\n  \tx\n\t\ty z\n",
    "\tfoo bar\n      x\n   \tend\n",
];

/// Prefixes each key sequence with `:set ts=N` for N in 2, 4 and 8.
fn each_ts(keys: &[&str]) -> Vec<String> {
    [2, 4, 8]
        .iter()
        .flat_map(|ts| keys.iter().map(move |k| format!(":set ts={ts}<CR>{k}")))
        .collect()
}

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

#[test]
fn tabstop_display_columns() {
    let keys = each_ts(&[
        "j$", "jj$", "jlll|", "3|", "jllj", "jjjk", "j0j", "$k", "jwj", "jfbl", "2j3l$", "jx",
    ]);
    check(&cross(TEXTS, &refs(&keys)));
}

#[test]
fn tabstop_visual_block() {
    let keys = each_ts(&[
        "j<C-v>jld",
        "j<C-v>j$d",
        "l<C-v>jlx",
        "w<C-v>jlcZ<Esc>",
        "j<C-v>jlU",
        "j<C-v>2jl~",
        "<C-v>j$A!<Esc>",
        "j<C-v>jIZ<Esc>",
    ]);
    check(&cross(TEXTS, &refs(&keys)));
    let keys = [":set ts=4<CR>lj<C-v>jlyP", ":set ts=8<CR>lj<C-v>jlyP"];
    check(&cross(TEXTS, &keys));
}

/// Known differences from Vim (not yet matched; the cases are kept here so
/// they run with `cargo test -- --ignored`).
#[test]
#[ignore = "known gap: block yank and put with 'tabstop' 2 and tabs (see PLAN.md)"]
fn known_gap_visual_block_put_tabstop_2() {
    check(&cross(TEXTS, &[":set ts=2<CR>lj<C-v>jlyP"]));
}

#[test]
#[ignore = "known gap: block '<' with a tab in the block (see PLAN.md)"]
fn known_gap_block_shift_left() {
    let keys = each_ts(&["j<C-v>j<"]);
    check(&cross(INDENT, &refs(&keys)));
}

#[test]
#[ignore = "known gap: Insert Tab with 'softtabstop' and 'tabstop' 2 or 4 (see PLAN.md)"]
fn known_gap_softtabstop_insert_tabstop_not_8() {
    const OPTS: &[&str] = &["sts=2 ts=4", "sts=4 ts=2", "sta sw=4 ts=2 sts=4"];
    let keys: Vec<String> = OPTS
        .iter()
        .flat_map(|o| SOFT_KEYS.iter().map(move |k| format!(":set {o}<CR>{k}")))
        .collect();
    check(&cross(INDENT, &refs(&keys)));
}

#[test]
#[ignore = "known gap: Replace-mode Tab with 'softtabstop' and noexpandtab (see PLAN.md)"]
fn known_gap_replace_mode_tab_noet() {
    let keys = [
        ":set sts=4<CR>R<Tab><Esc>",
        ":set sts=4<CR>R<Tab><Tab><Esc>",
        ":set sts=4 et<CR>R<Tab><Tab><BS><BS><Esc>",
        ":set sts=4<CR>R<Tab><Tab><BS><BS><Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn tabstop_shifting() {
    let keys = each_ts(&[
        ">>",
        "j>>",
        "3>>",
        "j<<",
        "j2<<",
        ">>j.",
        "V>",
        "<C-v>j>",
        ":set sw=0<CR>>>",
        ":set sw=0<CR>j<<",
        ":set sw=0 et<CR>>>",
        ":set sw=0 et<CR>j<<",
    ]);
    check(&cross(INDENT, &refs(&keys)));
}

#[test]
fn shiftround_shifting() {
    let keys = [
        ":set sr sw=4<CR>>>",
        ":set sr sw=4<CR><<",
        ":set sr sw=4<CR>j>>",
        ":set sr sw=4<CR>j<<",
        ":set sr sw=4<CR>3>>",
        ":set sr sw=4<CR>3<<",
        ":set sr sw=4 ts=4<CR>>>",
        ":set sr sw=4 ts=4 et<CR>j<<",
        ":set sr sw=3<CR>>>",
        ":set sr sw=3<CR>j<<",
        ":set sr sw=2<CR>j<<",
        ":set sr sw=0 ts=2<CR>>>",
        ":set sr sw=0 ts=2<CR>j<<",
        ":set nosr sw=4<CR>>>",
        ":set sr sw=4<CR>Vj>",
        ":set sr sw=4<CR>Vj<",
        ":set sr<CR>:set sw&<CR>>>",
    ];
    check(&cross(INDENT, &keys));
}

/// Option sets for the Insert-mode Tab and BS keys.
const SOFT_OPTS: &[&str] = &[
    "sts=4",
    "sts=4 et",
    "sts=3",
    "sts=-1 sw=4",
    "sts=-1 sw=4 et",
    "sta sw=4",
    "sta sw=4 et",
    "sta sw=8",
    "sta sw=4 sts=4",
    "sts=8 et",
];

const SOFT_KEYS: &[&str] = &[
    "i<Tab><Esc>",
    "i<Tab><Tab><Esc>",
    "i<Tab><Tab><Tab><Esc>",
    "A<Tab><Esc>",
    "A<Tab><Tab><Esc>",
    "A<Tab><Tab><BS><Esc>",
    "A<Tab><Tab><BS><BS><Esc>",
    "A<Tab><Tab><BS><BS><BS><Esc>",
    "A<Tab><Tab><Tab><BS><Esc>",
    "i<Tab><BS><Esc>",
    "i<Tab><Tab><BS><BS><Esc>",
    "i  <BS><Esc>",
    "i    <BS><BS><Esc>",
    "i<Tab><Space><BS><Esc>",
    "i<Tab><Space><BS><BS><Esc>",
    "jI<Tab><Esc>",
    "jI<Tab><Tab><BS><Esc>",
    "jI<Tab><Tab><Tab><BS><BS><Esc>",
    "j$i<BS><Esc>",
    "j$i<BS><BS><Esc>",
    "jA<Tab><Tab><Esc>",
    "j3i<Tab><Esc>",
    "jAx<Tab><Tab><Esc>",
    "ji<Tab><Tab><Esc>",
    "j2li<Tab><Esc>",
    "j3li<Tab><Esc>",
    "j3li<BS><Esc>",
    "j$i<Tab><Esc>",
    "3i<Tab><Esc>",
];

#[test]
fn softtabstop_and_smarttab_insert() {
    let keys: Vec<String> = SOFT_OPTS
        .iter()
        .flat_map(|o| SOFT_KEYS.iter().map(move |k| format!(":set {o}<CR>{k}")))
        .collect();
    check(&cross(INDENT, &refs(&keys)));
}

#[test]
fn softtabstop_insert_with_tabstop() {
    const OPTS: &[&str] = &["sts=4", "sts=4 et", "sts=-1 sw=4", "sta sw=4"];
    let keys: Vec<String> = [2, 4, 8]
        .iter()
        .flat_map(|ts| {
            OPTS.iter().flat_map(move |o| {
                SOFT_KEYS
                    .iter()
                    .map(move |k| format!(":set ts={ts} {o}<CR>{k}"))
            })
        })
        .collect();
    check(&cross(TEXTS, &refs(&keys)));
}

#[test]
fn expandtab_with_tabstop() {
    let keys = each_ts(&[
        ":set et<CR>i<Tab><Esc>",
        ":set et<CR>A<Tab><Tab><Esc>",
        ":set et<CR>jI<Tab>x<Esc>",
        ":set et<CR>>>",
        ":set et<CR>j>>",
        ":set et<CR>wi<Tab><Esc>",
        ":set noet<CR>i<Tab><Esc>",
        ":set noet<CR>jA<Tab><Tab><Esc>",
    ]);
    check(&cross(INDENT, &refs(&keys)));
}

#[test]
fn replace_mode_tab_with_softtabstop() {
    let keys = [
        ":set sts=4 et<CR>R<Tab><Esc>",
        ":set sts=4 et<CR>R<Tab><Tab><Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn tabstop_and_softtabstop_set_errors() {
    let keys = [
        ":set ts=0<CR>>>",
        ":set ts=-1<CR>>>",
        ":set ts=4 ts+=2<CR>>>",
        ":set ts=4 ts-=4<CR>>>",
        ":set sts=-2<CR>i<Tab><Esc>",
        ":set sts=-1<CR>i<Tab><Esc>",
        ":set sts+=2<CR>i<Tab><Esc>",
        ":set sr?<CR>>>",
        ":set sta!<CR>i<Tab><Esc>",
        ":set invsta<CR>i<Tab><Esc>",
    ];
    check(&cross(INDENT, &keys));
}
